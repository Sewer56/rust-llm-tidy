//! Match syntax-only symbol policies and return hints separately from exclusions.
//!
//! Literal names match trailing `::` components; regexes match whole written
//! paths. Rust methods expose their method name, not the receiver's type.
//!
//! C# invocation receivers retain written qualification where it is a name.
//! Imports, aliases, overloads, inferred types, and macro expansions are not
//! resolved. Comments and string contents are not symbol occurrences.

use crate::config::{
    ArrayKind, CompiledSymbolRule, ReportingScope, SymbolAction, SymbolLanguage, SymbolMatcher,
    SymbolTarget,
};
use crate::reporting::Diagnostic;
use crate::source::{ParseResult, symbols::declarations};
use core::ops::{Range, RangeInclusive};
use usage::Usage;

#[cfg(test)]
mod array_tests;
pub(crate) mod builtins;
pub(crate) mod legacy;
#[cfg(test)]
mod tests;
mod usage;

/// Independent outputs: callers apply exclusions even when SYM001 is disabled.
#[derive(Debug, Default)]
pub(crate) struct SymbolObservations {
    pub(crate) hints: Vec<HintObservation>,
    pub(crate) excluded_ranges: Vec<Range<usize>>,
}

/// One hint and its reporting boundary, without changing `Diagnostic`.
#[derive(Debug)]
pub(crate) struct HintObservation {
    pub(crate) diagnostic: Diagnostic,
    pub(crate) scope: Option<ReportingScope>,
}

/// Evaluate ordered hints and every declaration exclusion for one parsed file.
///
/// First matching hint wins per occurrence. Exclusions do not consume that
/// first-match slot and are collected regardless of their position. The caller
/// owns lint enablement, line-scope filtering, and exclusion application.
///
/// # Errors
/// Returns an error when declaration policies need a supported language's tree
/// that contains syntax errors or missing tokens.
///
/// Stop protected processing
/// rather than treating the missing exclusion spans as permission to edit.
pub(crate) fn check(
    parsed: &ParseResult,
    ext: &str,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<SymbolObservations> {
    let mut result = SymbolObservations::default();
    let Some(language) = SymbolLanguage::for_extension(ext) else {
        return Ok(result);
    };
    let applicable: Vec<_> = rules
        .iter()
        .filter(|rule| rule.applies_to(language))
        .collect();
    if applicable.is_empty() {
        return Ok(result);
    }

    if applicable
        .iter()
        .any(|rule| rule.target == SymbolTarget::Declaration)
    {
        for declaration in declarations(parsed, ext)? {
            let matching = || {
                applicable.iter().copied().filter(|rule| {
                    rule.target == SymbolTarget::Declaration
                        && matches_name(&rule.matcher, &declaration.path)
                })
            };
            if matching().any(|rule| rule.action == SymbolAction::Exclude) {
                result.excluded_ranges.push(declaration.bytes);
            }
            if let Some(rule) = matching().find(|rule| rule.action == SymbolAction::Hint) {
                result.hints.push(observation(
                    rule,
                    &declaration.path,
                    "declaration",
                    declaration.name_lines,
                ));
            }
        }
    }

    if applicable
        .iter()
        .any(|rule| rule.target == SymbolTarget::Usage)
    {
        let mut cursor = parsed.syntax_tree().root_node().walk();
        loop {
            if let Some(usage) = usage::extract(cursor.node(), parsed.source.as_str(), language)
                && let Some(rule) = applicable
                    .iter()
                    .copied()
                    .find(|rule| matches_usage(rule, &usage, language))
            {
                let mut hint = observation(rule, &usage.path, usage.kind, usage.name_lines.clone());
                if matches!(rule.matcher, SymbolMatcher::Legacy { .. }) {
                    hint.diagnostic.item_name = Some(usage.legacy_name.to_owned());
                }
                result.hints.push(hint);
            }
            if cursor.goto_first_child() {
                continue;
            }

            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return Ok(result);
                }
            }
        }
    }
    Ok(result)
}

/// Extract only declaration exclusions, without scanning invocation usages.
///
/// # Errors
/// Returns an error when applicable exclusions encounter a syntax-error tree.
pub(crate) fn excluded_ranges(
    parsed: &ParseResult,
    ext: &str,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<Vec<Range<usize>>> {
    let Some(language) = SymbolLanguage::for_extension(ext) else {
        return Ok(Vec::new());
    };
    let exclusions: Vec<_> = rules
        .iter()
        .filter(|rule| rule.action == SymbolAction::Exclude && rule.applies_to(language))
        .collect();
    if exclusions.is_empty() {
        return Ok(Vec::new());
    }

    Ok(declarations(parsed, ext)?
        .into_iter()
        .filter(|declaration| {
            exclusions
                .iter()
                .any(|rule| matches_name(&rule.matcher, &declaration.path))
        })
        .map(|declaration| declaration.bytes)
        .collect())
}

/// Apply explicit invocation constraints before comparing names.
fn matches_usage(rule: &CompiledSymbolRule, usage: &Usage<'_>, language: SymbolLanguage) -> bool {
    let zero_arguments = if matches!(rule.matcher, SymbolMatcher::Legacy { .. }) {
        usage.legacy_zero_arguments
    } else {
        usage.zero_arguments
    };

    if rule.target != SymbolTarget::Usage
        || rule.action != SymbolAction::Hint
        || rule.array_kind.is_some_and(|kind| match kind {
            ArrayKind::Any => usage.array_kind.is_none(),
            ArrayKind::ExplicitSizedVector => usage.array_kind != Some(kind),
        })
        || rule
            .zero_arguments
            .is_some_and(|value| value != zero_arguments)
        || rule
            .no_initializer
            .is_some_and(|value| value != usage.no_initializer)
    {
        return false;
    }

    match &rule.matcher {
        SymbolMatcher::Legacy {
            pattern,
            components,
        } => legacy::matches(pattern, components, usage, language),
        matcher => matches_name(matcher, &usage.path),
    }
}

/// Render the selected message while preserving its per-rule scope override.
fn observation(
    rule: &CompiledSymbolRule,
    name: &str,
    kind: &str,
    name_lines: RangeInclusive<usize>,
) -> HintObservation {
    HintObservation {
        diagnostic: Diagnostic {
            severity: rule.severity,
            code: rule.code,
            message: rule.message.as_deref().unwrap_or_default().to_owned(),
            line: *name_lines.end(),
            item_kind: kind.to_owned(),
            item_name: Some(name.to_owned()),
        },
        scope: rule.scope,
    }
}

/// Compare a normalized name, keeping literal component boundaries intact.
fn matches_name(matcher: &SymbolMatcher, name: &str) -> bool {
    match matcher {
        SymbolMatcher::Literal(symbol) => {
            name == symbol.as_ref()
                || name
                    .strip_suffix(symbol.as_ref())
                    .is_some_and(|prefix| prefix.ends_with("::"))
        }
        SymbolMatcher::Regex(regex) => regex.is_match(name),
        SymbolMatcher::Legacy { .. } => false,
    }
}
