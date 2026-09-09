//! Match syntax-only symbol policies and return hints separately from exclusions.
//!
//! Literal names match trailing `::` components; regexes match whole written
//! declaration paths. Usage regexes search original text in selected comment
//! regions. Rust methods expose their method name, not the receiver's type.
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
#[cfg(test)]
mod tests;
pub(crate) mod text_regex;
#[cfg(test)]
mod text_regex_tests;
mod usage;

/// Independent outputs: callers apply exclusions even when SYM is disabled.
#[derive(Debug, Default)]
pub(crate) struct SymbolObservations {
    pub(crate) warnings: Vec<String>,
    pub(crate) hints: Vec<HintObservation>,
    /// Edit protection, independent of lint suppression.
    pub(crate) excluded_ranges: Vec<Range<usize>>,
    /// Declaration byte ranges paired with registry-ordered lint-code masks.
    pub(crate) lint_exclusions: Vec<(Range<usize>, u32)>,
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
    check_enabled(parsed, ext, rules, true)
}

/// Extract only edit-excluded declarations, without scanning invocation usages.
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
        .filter(|rule| {
            rule.action == SymbolAction::Exclude && rule.exclude_edits && rule.applies_to(language)
        })
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

/// Check file-wide post-processing opt-outs without applying edit exclusions.
///
/// The caller supplies only opt-outs applicable to this file's language.
///
/// # Errors
/// Returns an error when applicable opt-outs encounter syntax errors or missing
/// tokens, even if no complete declaration matches.
pub(crate) fn excludes_post_process(
    parsed: &ParseResult,
    ext: &str,
    rules: &[&CompiledSymbolRule],
) -> anyhow::Result<bool> {
    Ok(declarations(parsed, ext)?.iter().any(|declaration| {
        rules
            .iter()
            .any(|rule| matches_name(&rule.matcher, &declaration.path))
    }))
}

/// Observe exclusions without scanning hints when SYM is disabled.
///
/// # Errors
/// Returns an error when applicable declaration policies encounter syntax errors
/// or missing tokens.
pub(crate) fn check_enabled(
    parsed: &ParseResult,
    ext: &str,
    rules: &[CompiledSymbolRule],
    hints_enabled: bool,
) -> anyhow::Result<SymbolObservations> {
    let mut result = SymbolObservations::default();
    let Some(language) = SymbolLanguage::for_extension(ext) else {
        return Ok(result);
    };
    let applicable: Vec<_> = rules
        .iter()
        .filter(|rule| {
            rule.applies_to(language) && (hints_enabled || rule.action == SymbolAction::Exclude)
        })
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
            let mut lint_mask = 0;
            let mut exclude_edits = false;
            for rule in matching().filter(|rule| rule.action == SymbolAction::Exclude) {
                lint_mask |= rule.exclude_lints;
                exclude_edits |= rule.exclude_edits;
            }
            if lint_mask != 0 {
                result
                    .lint_exclusions
                    .push((declaration.bytes.clone(), lint_mask));
            }
            if exclude_edits {
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
                if rule.capacity_reminder {
                    hint.diagnostic.item_name = Some(usage.written_name.to_owned());
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

/// Apply explicit invocation constraints before comparing names.
fn matches_usage(rule: &CompiledSymbolRule, usage: &Usage<'_>, language: SymbolLanguage) -> bool {
    if rule.target != SymbolTarget::Usage
        || rule.action != SymbolAction::Hint
        || rule.array_kind.is_some_and(|kind| match kind {
            ArrayKind::Any => usage.array_kind.is_none(),
            ArrayKind::ExplicitSizedVector => usage.array_kind != Some(kind),
        })
        || rule
            .zero_arguments
            .is_some_and(|value| value != usage.zero_arguments)
        || rule
            .no_initializer
            .is_some_and(|value| value != usage.no_initializer)
    {
        return false;
    }

    // PERF001 requires an empty argument list. Even comments make it nonempty.
    if rule.capacity_reminder
        && (!usage.empty_argument_list
            || usage.kind
                != match language {
                    SymbolLanguage::Rust => "call",
                    SymbolLanguage::Csharp => "creation",
                })
    {
        return false;
    }

    matches_name(&rule.matcher, &usage.path)
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
            title: rule.title.clone(),
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
    }
}
