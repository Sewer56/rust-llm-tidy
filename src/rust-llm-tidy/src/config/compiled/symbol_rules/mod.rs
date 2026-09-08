//! Validate symbol policies and compile bounded whole-name matchers once.

use crate::config::{
    ArrayKind, ReportingScope, SymbolAction, SymbolLanguage, SymbolRule, SymbolTarget,
};
use crate::reporting::Severity;
use crate::rules::registry::CODE_SYM001;
use anyhow::{Context, bail};
use regex::{Regex, RegexBuilder};

#[cfg(test)]
mod tests;

/// Maximum UTF-8 bytes in one hint message.
pub(crate) const MAX_MESSAGE_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 bytes in one literal or regex pattern.
pub(crate) const MAX_PATTERN_BYTES: usize = 4096;
/// Maximum policies in one configured symbol list.
pub(crate) const MAX_SYMBOL_RULES: usize = 256;
/// Maximum parser nesting in a regex, before compilation.
const REGEX_NEST_LIMIT: u32 = 64;
/// Per-regex compiled program and lazy DFA cache budgets.
const REGEX_SIZE_LIMIT: usize = 256 * 1024;

/// A validated rule, retaining order and diagnostic provenance.
#[derive(Debug)]
pub(crate) struct CompiledSymbolRule {
    pub(crate) language: Option<SymbolLanguage>,
    pub(crate) extensions: [bool; 2],
    pub(crate) array_kind: Option<ArrayKind>,
    pub(crate) target: SymbolTarget,
    pub(crate) matcher: SymbolMatcher,
    pub(crate) action: SymbolAction,
    pub(crate) message: Option<Box<str>>,
    pub(crate) scope: Option<ReportingScope>,
    pub(crate) severity: Severity,
    pub(crate) zero_arguments: Option<bool>,
    pub(crate) no_initializer: Option<bool>,
    pub(crate) code: &'static str,
}

/// Precompiled name comparisons; legacy variants retain PERF001 semantics.
#[derive(Debug)]
pub(crate) enum SymbolMatcher {
    Literal(Box<str>),
    Regex(Regex),
    Legacy {
        pattern: Box<str>,
        components: Box<[Box<str>]>,
    },
}

impl CompiledSymbolRule {
    /// Intersect the legacy language selector with enabled source extensions.
    pub(crate) fn applies_to(&self, language: SymbolLanguage) -> bool {
        self.language.is_none_or(|value| value == language)
            && self.extensions[match language {
                SymbolLanguage::Rust => 0,
                SymbolLanguage::Csharp => 1,
            }]
    }
}

/// Compile policies without requiring matches in the current source tree.
///
/// # Errors
/// Returns an error for these policy problems:
/// - Rule count exceeds [`MAX_SYMBOL_RULES`].
/// - Patterns or messages are blank or exceed their byte limits.
/// - Literal components are malformed, or regex compilation exceeds syntax
///   or resource limits.
/// - Both matchers are present, or neither is present.
/// - Hint messages are missing, or fields conflict with the action or target.
/// - Extensions are empty, dotted, or not supported (`rs` and `cs`).
pub(crate) fn compile_symbol_rules(
    rules: &[SymbolRule],
) -> anyhow::Result<Vec<CompiledSymbolRule>> {
    if rules.len() > MAX_SYMBOL_RULES {
        bail!("symbol_rules exceeds {MAX_SYMBOL_RULES} entries; reduce the rule list");
    }

    rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            compile_rule(rule).with_context(|| format!("symbol_rules entry {}", index + 1))
        })
        .collect()
}

/// Bound user text before regex compilation or dependent allocation.
///
/// # Errors
/// Returns an error when the field exceeds its byte limit or is blank.
pub(crate) fn validate_text(text: &str, limit: usize, field: &str) -> anyhow::Result<()> {
    if text.len() > limit {
        bail!("{field} exceeds {limit} bytes; shorten it");
    }
    if text.trim().is_empty() {
        bail!("{field} must not be empty; provide nonblank text");
    }
    Ok(())
}

/// Reject incompatible policy fields before compiling any matcher.
fn compile_rule(rule: &SymbolRule) -> anyhow::Result<CompiledSymbolRule> {
    let mut extensions = [true; 2];
    if let Some(values) = &rule.extensions {
        if values.is_empty() {
            bail!("extensions must not be empty; omit it to enable supported languages");
        }
        extensions = [false; 2];
        for value in values {
            let index = match SymbolLanguage::for_extension(value) {
                Some(SymbolLanguage::Rust) => 0,
                Some(SymbolLanguage::Csharp) => 1,
                None => bail!("extensions must contain rs or cs without leading dots"),
            };
            extensions[index] = true;
        }
    }

    let usage_constraints =
        rule.zero_arguments.is_some() || rule.no_initializer.is_some() || rule.array_kind.is_some();
    if rule.target == SymbolTarget::Declaration && usage_constraints {
        bail!(
            "declaration rules cannot have zero_arguments, no_initializer, or array_kind; remove them"
        );
    }
    match rule.action {
        SymbolAction::Hint => {
            let message = rule.message.as_deref().unwrap_or_default();
            validate_text(message, MAX_MESSAGE_BYTES, "message")?;
        }
        SymbolAction::Exclude => {
            if rule.target != SymbolTarget::Declaration
                || rule.message.is_some()
                || rule.scope.is_some()
                || usage_constraints
            {
                bail!(
                    "exclude requires target: declaration and forbids message, scope, and usage constraints"
                );
            }
        }
    }

    let matcher = match (&rule.symbol, &rule.regex) {
        (Some(symbol), None) => {
            validate_text(symbol, MAX_PATTERN_BYTES, "symbol")?;
            if !valid_literal(symbol) {
                bail!(
                    "symbol must contain nonempty literal components separated by ::; use regex for patterns"
                );
            }
            SymbolMatcher::Literal(symbol.clone())
        }
        (None, Some(pattern)) => {
            validate_text(pattern, MAX_PATTERN_BYTES, "regex")?;
            let regex = RegexBuilder::new(&format!(r"\A(?:{pattern})\z"))
                .size_limit(REGEX_SIZE_LIMIT)
                .dfa_size_limit(REGEX_SIZE_LIMIT)
                .nest_limit(REGEX_NEST_LIMIT)
                .build()
                .context("regex failed bounded compilation; simplify its syntax or size")?;
            SymbolMatcher::Regex(regex)
        }
        _ => bail!("set exactly one of symbol and regex"),
    };

    Ok(CompiledSymbolRule {
        language: rule.language,
        extensions,
        array_kind: rule.array_kind,
        target: rule.target,
        matcher,
        action: rule.action,
        message: rule.message.clone(),
        scope: rule.scope,
        severity: rule.severity.unwrap_or(Severity::Reminder),
        zero_arguments: rule.zero_arguments,
        no_initializer: rule.no_initializer,
        code: CODE_SYM001,
    })
}

/// Check literal component shape without interpreting wildcard punctuation.
fn valid_literal(symbol: &str) -> bool {
    if symbol == "new[]" {
        return true;
    }
    let name = symbol.strip_suffix('!').unwrap_or(symbol);
    name.split("::").all(|part| {
        let part = part
            .strip_prefix("r#")
            .or_else(|| part.strip_prefix('@'))
            .unwrap_or(part);
        let mut chars = part.chars();
        chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
            && chars.all(|c| c == '_' || c.is_alphanumeric())
    })
}
