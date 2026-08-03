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
//! | `DOC006`  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`/...).    |
//! | `TEST001` | Warning  | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |

use crate::diagnostic::Diagnostic;
use rust_llm_tidy_model::parse::ParseResult;

mod arguments;
mod docs;
mod errors;
mod placeholder;
mod test_naming;

pub use arguments::{missing_arguments_section, undocumented_param};
pub use docs::missing_docs;
pub use errors::{missing_errors_section, vague_errors};
pub use placeholder::doc_placeholder;
pub use test_naming::test_naming;

use rust_llm_tidy_model::parse::ItemKind;

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
    CODE_TEST_NAMING,
];
/// Rule code for placeholder text in doc comments.
pub const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for a missing `# Arguments` section.
pub const CODE_MISSING_ARGUMENTS: &str = "DOC004";
/// Rule code for missing doc comments.
pub const CODE_MISSING_DOCS: &str = "DOC001";
/// Rule code for a missing `# Errors` section.
pub const CODE_MISSING_ERRORS: &str = "DOC002";
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
    // Each item produces at most a handful of diagnostics; preallocate to the
    // item count to avoid regrowth on the common dirty-file path.
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
    fn lint_codes_lists_all_seven_codes() {
        // `LINT_CODES` is the source of truth for CLI rule validation. It must
        // enumerate every code produced by `run_all`.
        assert_eq!(
            LINT_CODES.len(),
            7,
            "LINT_CODES must list exactly seven codes: {LINT_CODES:?}"
        );
        for code in [
            CODE_MISSING_DOCS,
            CODE_MISSING_ERRORS,
            CODE_VAGUE_ERRORS,
            CODE_MISSING_ARGUMENTS,
            CODE_UNDOCUMENTED_PARAM,
            CODE_DOC_PLACEHOLDER,
            CODE_TEST_NAMING,
        ] {
            assert!(
                LINT_CODES.iter().any(|c| *c == code),
                "LINT_CODES is missing {code}"
            );
        }
    }
}
