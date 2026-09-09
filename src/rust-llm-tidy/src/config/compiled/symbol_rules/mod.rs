//! Validate text and symbol policies and compile matchers with regex defaults.

use crate::config::{
    ArrayKind, DeclarationLintExclusion, RegexComments, ReportingScope, SymbolAction,
    SymbolLanguage, SymbolRule, SymbolTarget,
};
use crate::languages::registry;
use crate::reporting::Severity;
use crate::rules::registry::{CODE_SYM, LINT_CODES};
use anyhow::{Context, bail};
use regex::Regex;

#[cfg(test)]
mod tests;

// Keep registry growth from silently dropping suppression bits.
const _: () = assert!(LINT_CODES.len() < u32::BITS as usize);

/// A validated rule, retaining order and diagnostic provenance.
#[derive(Debug)]
pub(crate) struct CompiledSymbolRule {
    pub(crate) extensions: [bool; 2],
    pub(crate) text_extensions: Option<Vec<Box<str>>>,
    pub(crate) comments: RegexComments,
    pub(crate) array_kind: Option<ArrayKind>,
    pub(crate) target: SymbolTarget,
    pub(crate) matcher: SymbolMatcher,
    pub(crate) action: SymbolAction,
    /// Bits in registry lint-code order; zero suppresses no diagnostics.
    pub(crate) exclude_lints: u32,
    pub(crate) exclude_edits: bool,
    pub(crate) exclude_post_process: bool,
    pub(crate) title: Option<Box<str>>,
    pub(crate) message: Option<Box<str>>,
    pub(crate) scope: Option<ReportingScope>,
    pub(crate) severity: Severity,
    pub(crate) zero_arguments: Option<bool>,
    pub(crate) no_initializer: Option<bool>,
    /// Preserve PERF001's written diagnostic names and empty argument-list syntax.
    pub(crate) capacity_reminder: bool,
    pub(crate) code: &'static str,
}

/// Precompiled comparisons against normalized symbol names.
#[derive(Debug)]
pub(crate) enum SymbolMatcher {
    Literal(Box<str>),
    Regex(Regex),
}

impl CompiledSymbolRule {
    /// Whether this policy searches text rather than parsed names.
    pub(crate) fn is_text_regex(&self) -> bool {
        self.target == SymbolTarget::Usage && matches!(self.matcher, SymbolMatcher::Regex(_))
    }

    /// Resolve text extensions independently of the parsed-name language selector.
    pub(crate) fn applies_to_extension(&self, ext: &str) -> bool {
        if self.is_text_regex() {
            registry::profile_for(ext).allows("lints")
                && self
                    .text_extensions
                    .as_ref()
                    .is_none_or(|values| values.iter().any(|value| value.eq_ignore_ascii_case(ext)))
        } else {
            SymbolLanguage::for_extension(ext).is_some_and(|language| self.applies_to(language))
        }
    }

    /// Test the compiled intersection of language and extension selectors.
    pub(crate) fn applies_to(&self, language: SymbolLanguage) -> bool {
        !self.is_text_regex()
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
/// - Patterns, titles, or messages are blank.
/// - Literal components are malformed, or regex syntax or dependency-default
///   compilation limits reject a pattern.
/// - Both matchers are present, or neither is present.
/// - Hint titles or messages are missing, or fields conflict with the action
///   or target.
/// - Language or extension selectors are empty, malformed, or unsupported.
/// - Text regexes have languages or usage constraints.
/// - Other matchers have comments.
/// - Exclusion controls appear on hints, or `exclude_lints` lists an unknown
///   lint code (including operation names and retired codes).
pub(crate) fn compile_symbol_rules(
    rules: &[SymbolRule],
) -> anyhow::Result<Vec<CompiledSymbolRule>> {
    rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            compile_rule(rule).with_context(|| format!("symbol_rules entry {}", index + 1))
        })
        .collect()
}

/// Reject incompatible policy fields before compiling any matcher.
fn compile_rule(rule: &SymbolRule) -> anyhow::Result<CompiledSymbolRule> {
    let text_regex = rule.regex.is_some() && rule.target == SymbolTarget::Usage;
    if text_regex && rule.languages.is_some() {
        bail!("languages cannot restrict text regexes; use extensions instead");
    }
    if !text_regex && rule.comments.is_some() {
        bail!("comments is only allowed on usage regexes; remove it");
    }

    let mut extensions = [true; 2];
    if let Some(values) = &rule.extensions {
        if values.is_empty() {
            bail!("extensions must not be empty; omit it to enable supported languages");
        }
        extensions = [false; 2];
        for value in values {
            if text_regex {
                registry::validate_extension(value)?;
                if !registry::profile_for(value).allows("lints") {
                    bail!("extensions must contain supported text extensions without leading dots");
                }
                continue;
            }
            let index = match SymbolLanguage::for_extension(value) {
                Some(SymbolLanguage::Rust) => 0,
                Some(SymbolLanguage::Csharp) => 1,
                None => bail!("extensions must contain rs or cs without leading dots"),
            };
            extensions[index] = true;
        }
    }

    if let Some(languages) = &rule.languages {
        if languages.is_empty() {
            bail!("languages must not be empty; omit it to enable both languages");
        }
        extensions[0] &= languages.contains(&SymbolLanguage::Rust);
        extensions[1] &= languages.contains(&SymbolLanguage::Csharp);
    }

    let usage_constraints =
        rule.zero_arguments.is_some() || rule.no_initializer.is_some() || rule.array_kind.is_some();
    if text_regex && usage_constraints {
        bail!(
            "text regexes cannot have zero_arguments, no_initializer, or array_kind; remove them"
        );
    }
    if rule.target == SymbolTarget::Declaration && usage_constraints {
        bail!(
            "declaration rules cannot have zero_arguments, no_initializer, or array_kind; remove them"
        );
    }
    match rule.action {
        SymbolAction::Hint => {
            if rule.exclude_lints.is_some()
                || rule.exclude_edits.is_some()
                || rule.exclude_post_process.is_some()
            {
                bail!(
                    "exclude_lints, exclude_edits, and exclude_post_process require action: exclude; remove them from hints"
                );
            }
            let message = rule.message.as_deref().unwrap_or_default();
            validate_text(message, "message")?;
            validate_text(rule.title.as_deref().unwrap_or_default(), "title")?;
        }
        SymbolAction::Exclude => {
            if rule.target != SymbolTarget::Declaration
                || rule.message.is_some()
                || rule.title.is_some()
                || rule.scope.is_some()
                || usage_constraints
            {
                bail!(
                    "exclude requires target: declaration and forbids title, message, scope, and usage constraints"
                );
            }
        }
    }

    let exclude_lints = match &rule.exclude_lints {
        None | Some(DeclarationLintExclusion::All(true)) => (1 << LINT_CODES.len()) - 1,
        Some(DeclarationLintExclusion::All(false)) => 0,
        Some(DeclarationLintExclusion::Codes(codes)) => {
            let mut mask = 0;
            for code in codes {
                let Some(index) = LINT_CODES.iter().position(|known| *known == code.as_ref())
                else {
                    bail!(
                        "exclude_lints contains unknown lint code {code:?}; use registered lint codes only"
                    );
                };
                mask |= 1 << index;
            }
            mask
        }
    };

    let matcher = match (&rule.symbol, &rule.regex) {
        (Some(symbol), None) => {
            validate_text(symbol, "symbol")?;
            if !valid_literal(symbol) {
                bail!(
                    "symbol must contain nonempty literal components separated by ::; use regex for patterns"
                );
            }
            SymbolMatcher::Literal(symbol.clone())
        }
        (None, Some(pattern)) => {
            validate_text(pattern, "regex")?;
            let anchored;
            let pattern = if text_regex {
                pattern.as_ref()
            } else {
                anchored = format!(r"\A(?:{pattern})\z");
                &anchored
            };
            let regex = Regex::new(pattern)
                .context("regex compilation failed; correct its syntax or simplify the pattern")?;
            SymbolMatcher::Regex(regex)
        }
        _ => bail!("set exactly one of symbol and regex"),
    };

    Ok(CompiledSymbolRule {
        extensions,
        text_extensions: text_regex.then(|| rule.extensions.clone()).flatten(),
        comments: rule.comments.unwrap_or_default(),
        array_kind: rule.array_kind,
        target: rule.target,
        matcher,
        action: rule.action,
        exclude_lints,
        exclude_edits: rule.exclude_edits.unwrap_or(true),
        exclude_post_process: rule.exclude_post_process.unwrap_or(false),
        title: rule.title.clone(),
        message: rule.message.clone(),
        scope: rule.scope,
        severity: rule.severity.unwrap_or(Severity::Reminder),
        zero_arguments: rule.zero_arguments,
        no_initializer: rule.no_initializer,
        capacity_reminder: false,
        code: CODE_SYM,
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

/// Reject blank policy fields without changing their supplied text.
///
/// # Errors
/// Returns an error when the field is blank.
fn validate_text(text: &str, field: &str) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        bail!("{field} must not be empty; provide nonblank text");
    }
    Ok(())
}
