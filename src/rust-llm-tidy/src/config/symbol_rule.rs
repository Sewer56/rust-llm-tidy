//! User-defined hints and declaration exclusions matched by written symbol names.

use super::ReportingScope;
use crate::reporting::Severity;
use serde::Deserialize;

/// A syntax-only symbol policy, without type or import resolution.
///
/// Set exactly one of `symbol` and `regex`. Literal names match trailing
/// `::`-separated components; regexes match the entire normalized name.
///
/// C# qualification uses `::` too. Generic arguments are omitted.
/// Receiver types and imported aliases are not resolved.
///
/// C# array creations use the synthetic name `new[]`, including `new T[n]`,
/// `new T[] { ... }`, `new[] { ... }`, multidimensional and jagged arrays.
///
/// Array declarations, collection expressions, and `stackalloc` are not usages.
/// The diagnostic anchors to the `new` token, not the size or initializer.
///
/// # Array reminder configuration
///
/// ```yaml
/// symbol_rules:
///   - symbol: new[]
///     extensions: [CS]
///     array_kind: explicit_sized_vector
///     no_initializer: true
///     message: "Review whether zero initialization is required."
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolRule {
    /// Restrict matching to one language; absent matches both.
    pub language: Option<SymbolLanguage>,
    /// Enabled source extensions, case insensitive and without dots: `rs`, `cs`.
    /// Absent enables both; an empty list or unsupported entry is an error.
    ///
    /// When `language` is also set, both restrictions must match.
    pub extensions: Option<Vec<Box<str>>>,
    /// Restrict usage hints to a C# array shape; absent allows any usage kind.
    pub array_kind: Option<ArrayKind>,
    /// Match an invocation by default, or a declaration when selected.
    #[serde(default)]
    pub target: SymbolTarget,
    /// Literal suffix of a normalized symbol path, such as `Vec::new`.
    pub symbol: Option<Box<str>>,
    /// Explicit regular expression, compiled with whole-name anchoring.
    pub regex: Option<Box<str>>,
    /// Emit a hint by default, or exclude a declaration from processing.
    #[serde(default)]
    pub action: SymbolAction,
    /// Required nonblank text for hints; forbidden for exclusions.
    pub message: Option<Box<str>>,
    /// Override the lint's reporting scope for this hint only.
    pub scope: Option<ReportingScope>,
    /// Hint severity; absent uses [`Severity::Reminder`]. Exclusions ignore it.
    pub severity: Option<Severity>,
    /// For usage hints, require zero arguments (`true`) or nonzero (`false`).
    /// Absent imposes no argument constraint. Arrays have no call arguments;
    /// their sizes and initializer elements do not count as arguments.
    pub zero_arguments: Option<bool>,
    /// For usage hints, require no initializer (`true`) or one (`false`).
    /// Absent imposes no initializer constraint.
    pub no_initializer: Option<bool>,
}

/// C# array syntax selected independently of its normalized `new[]` name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrayKind {
    /// Any explicit or implicit array creation, including initializers and ranks.
    Any,
    /// Explicit `new T[length]`, with one rank and a non-array element type.
    /// Initializers remain eligible unless `no_initializer: true` is set.
    ExplicitSizedVector,
}

/// Effect of a matched symbol rule.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolAction {
    /// Emit a configurable SYM001 diagnostic, defaulting to reminder severity.
    #[default]
    Hint,
    /// Exclude a declaration regardless of lint enablement or reporting scope.
    Exclude,
}

/// Parser language for a symbol policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolLanguage {
    /// Rust source.
    Rust,
    /// C# source.
    Csharp,
}

/// Syntax occurrences eligible for a symbol rule.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolTarget {
    /// Calls, methods, Rust macros, explicit C# objects, and C# array creations.
    #[default]
    Usage,
    /// Named declarations, including their owned documentation and attributes.
    Declaration,
}

impl SymbolLanguage {
    /// Resolve supported source extensions without case sensitivity.
    pub(crate) fn for_extension(ext: &str) -> Option<Self> {
        if ext.eq_ignore_ascii_case("rs") {
            Some(Self::Rust)
        } else if ext.eq_ignore_ascii_case("cs") {
            Some(Self::Csharp)
        } else {
            None
        }
    }
}
