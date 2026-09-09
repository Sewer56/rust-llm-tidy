//! User-defined text hints, symbol hints, and declaration exclusions.

use super::ReportingScope;
use crate::reporting::Severity;
use serde::Deserialize;

/// A syntax-only symbol policy, without type or import resolution.
///
/// Set exactly one of `symbol` and `regex`. Literal names match trailing
/// `::`-separated components. Usage regexes search text; declaration regexes
/// match the entire normalized declaration name.
///
/// C# qualification uses `::` too. Generic arguments are omitted.
/// Receiver types and imported aliases are not resolved.
///
/// Configuration is trusted: rule counts and text lengths have no application
/// limits. Regex compilation retains the [`regex`] crate's defaults.
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
///     title: Array initialization
///     extensions: [CS]
///     array_kind: explicit_sized_vector
///     no_initializer: true
///     message: "Review whether zero initialization is required."
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolRule {
    /// Restrict parsed names to `rust` and/or `csharp`; absent matches both.
    /// Empty lists, unsupported languages, and use with text regexes are errors.
    pub languages: Option<Vec<SymbolLanguage>>,
    /// Enabled source extensions, case insensitive and without dots.
    ///
    /// Text regexes accept supported text languages; parsed names accept `rs`, `cs`.
    /// Absent enables all applicable extensions; empty or unsupported lists fail.
    ///
    /// When `languages` is also set, both restrictions must match.
    pub extensions: Option<Vec<Box<str>>>,
    /// Restrict usage hints to a C# array shape; absent allows any usage kind.
    pub array_kind: Option<ArrayKind>,
    /// Match an invocation by default, or a declaration when selected.
    #[serde(default)]
    pub target: SymbolTarget,
    /// Literal suffix of a normalized symbol path, such as `Vec::new`.
    pub symbol: Option<Box<str>>,
    /// Text regex without implicit anchors for usage hints.
    ///
    /// Declaration targets match whole normalized names. Inline flags enable
    /// multiline matching.
    pub regex: Option<Box<str>>,
    /// Comment selection for usage regexes only; absent includes all text.
    /// `Exclude` and `Only` skip with a warning when comments cannot be identified.
    pub comments: Option<RegexComments>,
    /// Emit a hint by default, or exclude a declaration from processing.
    #[serde(default)]
    pub action: SymbolAction,
    /// Declaration-local lint suppression; absent means `true`.
    ///
    /// `false` and an empty list suppress nothing. Lists accept registered lint
    /// codes only. `MOD001` and `DOC009` remain file-level and are never suppressed.
    ///
    /// Overlapping exclusions add suppression; `false` cannot undo another rule.
    /// Forbidden on hints. Independent of [`Self::exclude_edits`].
    pub exclude_lints: Option<DeclarationLintExclusion>,
    /// Protect the declaration and its owned docs from built-in edits.
    ///
    /// Absent means `true`. Forbidden on hints. `false` permits built-in edits
    /// unless another rule protects them. External tools may edit declarations unless
    /// [`Self::exclude_post_process`] opts the file out.
    pub exclude_edits: Option<bool>,
    /// Skip external post-processing for a file with a matching declaration.
    ///
    /// Absent means `false`. Independent of edit and lint exclusion. Overlapping
    /// opt-outs add: `false` cannot undo another rule. Forbidden on hints.
    pub exclude_post_process: Option<bool>,
    /// Required nonblank hint title; forbidden for exclusions.
    pub title: Option<Box<str>>,
    /// Required nonblank text for hints; forbidden for exclusions.
    pub message: Option<Box<str>>,
    /// Override the lint's reporting scope for this hint only.
    /// [`crate::RunOptions::all_lines`] and [`crate::SourceOptions::all_lines`]
    /// take precedence.
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

/// Select all, none, or specific declaration-local lint codes for suppression.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DeclarationLintExclusion {
    /// `true` suppresses all declaration-local lints; `false` suppresses none.
    All(bool),
    /// Suppress only registered lint codes. An empty list suppresses none.
    Codes(Vec<Box<str>>),
}

/// Which original source regions a usage regex can search.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegexComments {
    /// Search all text, including comments and strings, without requiring a parser.
    #[default]
    Include,
    /// Search outside comments; matches cannot cross comment boundaries.
    Exclude,
    /// Search each comment including its delimiters, without crossing boundaries.
    Only,
}

/// Effect of a matched symbol rule.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolAction {
    /// Emit a configurable SYM diagnostic, defaulting to reminder severity.
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
