//! Documentation checks that run over a parsed source file.
//!
//! Each check is a pure function over a
//! [`rust_llm_tidy_model::parse::SourceItem`] that returns a
//! [`Vec<Diagnostic>`]. [`run_all`] runs every check and concatenates results.
//!
//! # Checks
//!
//! | Code      | Severity | Fires when                                                             |
//! | --------- | -------- | ---------------------------------------------------------------------- |
//! | `DOC001`  | Error    | A non-private item has no `///` doc comment.                           |
//! | `DOC002`  | Error    | A `pub fn` returning `Result` has no `# Errors` section.               |
//! | `DOC003`  | Warning  | A `# Errors` section names no concrete error variant.                  |
//! | `DOC004`  | Warning  | A `pub fn` with parameters has no `# Arguments` section.               |
//! | `DOC005`  | Warning  | A `# Arguments` section does not mention every parameter name.         |
//! | `DOC006`  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`).        |
//! | `DOC007`  | Error    | A doc paragraph over 240 chars of full text.                           |
//! | `DOC008`  | Warning  | A doc line over 80 chars of full text.                                 |
//! | `TEST001` | Warning  | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |
//!
//! Each code also carries a short friendly title; [`Diagnostic::title`]
//! exposes it for a finding.

use crate::diagnostic::Diagnostic;
pub use arguments::{missing_arguments_section, undocumented_param};
pub use docs::missing_docs;
pub use errors::{missing_errors_section, vague_errors};
pub use placeholder::doc_placeholder;
pub use plaintext::{
    Dialect, DocRegion, RegionLine, line_marker_regions, run_region_checks, run_text_checks,
};
use rust_llm_tidy_model::parse::ItemKind;
use rust_llm_tidy_model::parse::ParseResult;
pub use test_naming::test_naming;

mod arguments;
mod docs;
mod errors;
mod placeholder;
mod plaintext;
mod test_naming;

/// Friendly title per lint code, paired `(code, title)` in [`LINT_CODES`]
/// order.
///
/// Titles are the short human-readable names output consumers render next
/// to the code; they are static so a lookup allocates nothing.
pub(crate) const CODE_TITLES: &[(&str, &str)] = &[
    (CODE_MISSING_DOCS, "missing documentation"),
    (CODE_MISSING_ERRORS, "missing `# Errors` section"),
    (CODE_VAGUE_ERRORS, "vague `# Errors` section"),
    (CODE_MISSING_ARGUMENTS, "missing `# Arguments` section"),
    (CODE_UNDOCUMENTED_PARAM, "undocumented parameter"),
    (CODE_DOC_PLACEHOLDER, "placeholder text"),
    (CODE_PARAGRAPH_SIZE, "oversized paragraph"),
    (CODE_LINE_LENGTH, "long line"),
    (CODE_TEST_NAMING, "non-behavioral test name"),
];
/// All lint codes accepted through `include.rules`, `exclude.rules`,
/// `--include`, and `--exclude`, in the order they run. The CLI validates rule
/// names against this slice plus
/// `KNOWN_FIX_OPS` (defined in the CLI crate's `config` module).
pub const LINT_CODES: &[&str] = &[
    CODE_MISSING_DOCS,
    CODE_MISSING_ERRORS,
    CODE_VAGUE_ERRORS,
    CODE_MISSING_ARGUMENTS,
    CODE_UNDOCUMENTED_PARAM,
    CODE_DOC_PLACEHOLDER,
    CODE_PARAGRAPH_SIZE,
    CODE_LINE_LENGTH,
    CODE_TEST_NAMING,
];
/// Rule code for placeholder text in doc comments.
pub const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for an over-limit stripped doc line.
pub const CODE_LINE_LENGTH: &str = "DOC008";
/// Rule code for a missing `# Arguments` section.
pub const CODE_MISSING_ARGUMENTS: &str = "DOC004";
/// Rule code for missing doc comments.
pub const CODE_MISSING_DOCS: &str = "DOC001";
/// Rule code for a missing `# Errors` section.
pub const CODE_MISSING_ERRORS: &str = "DOC002";
/// Rule code for an over-limit paragraph of stripped doc text.
pub const CODE_PARAGRAPH_SIZE: &str = "DOC007";
/// Rule code for a discouraged test-function name.
pub const CODE_TEST_NAMING: &str = "TEST001";
/// Rule code for an undocumented parameter.
pub const CODE_UNDOCUMENTED_PARAM: &str = "DOC005";
/// Rule code for a vague `# Errors` section.
pub const CODE_VAGUE_ERRORS: &str = "DOC003";

/// Run every documentation check over `parsed` and return all diagnostics.
///
/// Diagnostics are returned in source order (by item, then by check). The
/// returned `Vec` is empty when every item passes every check.
///
/// # Arguments
///
/// - `parsed` - the parsed source result whose items are checked.
pub fn run_all(parsed: &ParseResult) -> Vec<Diagnostic> {
    // Each item produces at most a handful of diagnostics; preallocating to the
    // item count can reduce regrowth on the common dirty-file path.
    let mut diags = Vec::with_capacity(parsed.items.len());
    for item in &parsed.items {
        diags.extend(missing_docs(item));
        diags.extend(missing_errors_section(item));
        diags.extend(vague_errors(item));
        diags.extend(missing_arguments_section(item));
        diags.extend(undocumented_param(item));
        diags.extend(doc_placeholder(item));
        diags.extend(test_naming(item));
    }
    diags
}

/// Items that should be documented (everything except modules, imports,
/// impls, macros, macro invocations, uncategorized items, and extern crate).
///
/// `Mod` is excluded: modules are documented via `//!` inner docs that often
/// live in a separate file this single-file checker does not parse, so flagging
/// a bare `pub mod foo;` declaration would be a false positive.
///
/// Used by DOC001 ([`missing_docs`]) and DOC006 ([`doc_placeholder`]).
pub(crate) fn is_documentable(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Fn
            | ItemKind::Struct
            | ItemKind::Enum
            | ItemKind::Union
            | ItemKind::Type
            | ItemKind::Trait
            | ItemKind::Const
            | ItemKind::Static
    )
}

/// Lines belonging to a doc section body: everything after the header at
/// `start` up to the next `# ` section header or end of docs.
///
/// A section ends at any trimmed line starting with `# `; empty lines and
/// content lines within the section are retained.
///
/// Used by DOC003 ([`vague_errors`]) and DOC005 ([`undocumented_param`]).
pub(crate) fn section_body(docs: &[String], start: usize) -> Vec<&str> {
    docs[start + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|s| !s.trim().starts_with("# "))
        .collect()
}

/// Friendly title for `code`, or `None` when `code` is not a lint code.
pub(crate) fn title_for_code(code: &str) -> Option<&'static str> {
    CODE_TITLES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, title)| *title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_model::parse;
    use rust_llm_tidy_model::parse::SourceItem;

    /// Parse `source` and return its first [`SourceItem`].
    ///
    /// Shared by every check module's `#[cfg(test)] mod tests` so the parser
    /// fixture helper is defined exactly once rather than duplicated.
    pub(crate) fn parse_one(source: &str) -> SourceItem {
        let parsed = parse::parse_source(source).unwrap();
        parsed
            .items
            .into_iter()
            .next()
            .expect("expected at least one item")
    }

    // ── start_line propagation ──

    #[test]
    fn test_start_line_is_reported() {
        // Item starts on line 3 (after two doc lines); the reported diagnostic
        // line must equal the precomputed start line, not 1.
        let item = parse_one("\n\npub fn do_thing() {}");
        let diags = missing_docs(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn lint_codes_lists_all_nine_codes() {
        // `LINT_CODES` is the source of truth for CLI rule validation. It must
        // enumerate every code produced by `run_all` and `run_text_checks`.
        assert_eq!(
            LINT_CODES.len(),
            9,
            "LINT_CODES must list exactly nine codes: {LINT_CODES:?}"
        );
        for code in [
            CODE_MISSING_DOCS,
            CODE_MISSING_ERRORS,
            CODE_VAGUE_ERRORS,
            CODE_MISSING_ARGUMENTS,
            CODE_UNDOCUMENTED_PARAM,
            CODE_DOC_PLACEHOLDER,
            CODE_PARAGRAPH_SIZE,
            CODE_LINE_LENGTH,
            CODE_TEST_NAMING,
        ] {
            assert!(LINT_CODES.contains(&code), "LINT_CODES is missing {code}");
        }
    }

    /// The title table pairs every lint code with a non-empty title and
    /// holds no extra codes.
    #[test]
    fn code_titles_cover_exactly_the_nine_lint_codes() {
        assert_eq!(
            CODE_TITLES.len(),
            LINT_CODES.len(),
            "CODE_TITLES must pair exactly the nine lint codes"
        );
        for code in LINT_CODES {
            let title =
                title_for_code(code).unwrap_or_else(|| panic!("no title defined for {code}"));
            assert!(!title.is_empty(), "title for {code} must not be empty");
        }
    }
}
