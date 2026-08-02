//! Documentation checks that run over a parsed source file.
//!
//! Each check is a pure function over a [`SourceItem`] that returns a
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

use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{ParseResult, SourceItem};

mod arguments;
mod docs;
mod errors;
mod placeholder;
mod shared;
mod test_naming;

pub use arguments::{missing_arguments_section, undocumented_param};
pub use docs::missing_docs;
pub use errors::{missing_errors_section, vague_errors};
pub use placeholder::doc_placeholder;
pub use test_naming::test_naming;

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

    fn parse_one(source: &str) -> SourceItem {
        let parsed = parse::parse_source(source).unwrap();
        parsed
            .items
            .into_iter()
            .next()
            .expect("expected at least one item")
    }

    // ── DOC001: missing_docs ──

    #[test]
    fn test_missing_docs_pub_fn() {
        let item = parse_one("pub fn do_thing() {}");
        let diags = missing_docs(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_DOCS);
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn test_missing_docs_documented() {
        let item = parse_one("/// Does the thing.\npub fn do_thing() {}");
        assert!(missing_docs(&item).is_empty());
    }

    #[test]
    fn test_missing_docs_private_skipped() {
        let item = parse_one("fn helper() {}");
        assert!(missing_docs(&item).is_empty());
    }

    #[test]
    fn test_missing_docs_pub_struct() {
        let item = parse_one("pub struct Foo;");
        let diags = missing_docs(&item);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn test_missing_docs_pub_crate() {
        let item = parse_one("pub(crate) fn internal() {}");
        let diags = missing_docs(&item);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn test_missing_docs_test_mod_skipped() {
        let source = "#[cfg(test)]\npub mod tests {}";
        let item = parse_one(source);
        assert!(missing_docs(&item).is_empty());
    }

    #[test]
    fn test_missing_docs_use_skipped() {
        let item = parse_one("pub use std::io;");
        assert!(missing_docs(&item).is_empty());
    }

    // ── DOC002: missing_errors_section ──

    #[test]
    fn test_missing_errors_no_section() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        let diags = missing_errors_section(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ERRORS);
    }

    #[test]
    fn test_missing_errors_has_section() {
        let item = parse_one(
            "/// Loads a file.\n///\n/// # Errors\n///\n/// Returns nothing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(missing_errors_section(&item).is_empty());
    }

    #[test]
    fn test_missing_errors_not_result() {
        let item = parse_one("pub fn load() -> u32 { 0 }");
        assert!(missing_errors_section(&item).is_empty());
    }

    #[test]
    fn test_missing_errors_private_skipped() {
        let item = parse_one("fn load() -> Result<(), String> { Ok(()) }");
        assert!(missing_errors_section(&item).is_empty());
    }

    // ── DOC003: vague_errors ──

    #[test]
    fn test_vague_errors_no_variants() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns an error if loading fails.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        let diags = vague_errors(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn test_vague_errors_with_variants() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns [Error::NotFound] if missing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(vague_errors(&item).is_empty());
    }

    #[test]
    fn test_vague_errors_no_section_skipped() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        assert!(vague_errors(&item).is_empty());
    }

    // ── DOC004: missing_arguments_section ──

    #[test]
    fn test_missing_arguments_no_section() {
        let item = parse_one("/// Greets.\npub fn greet(name: &str) {}");
        let diags = missing_arguments_section(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ARGUMENTS);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn test_missing_arguments_has_section() {
        let item = parse_one(
            "/// Greets.\n///\n/// # Arguments\n///\n/// `name` - the name.\npub fn greet(name: &str) {}",
        );
        assert!(missing_arguments_section(&item).is_empty());
    }

    #[test]
    fn test_missing_arguments_accepts_all_header_variants() {
        // Every accepted alias should suppress DOC004 when params are documented.
        for header in [
            "# Arguments",
            "# Argument",
            "# Parameters",
            "# Parameter",
            "# Params",
            "# Param",
            // Case-insensitivity
            "# arguments",
            "# ARGUMENTS",
            "# pArAmS",
        ] {
            let source = format!(
                "/// Greets.\n///\n/// {header}\n///\n/// `name` - the name.\npub fn greet(name: &str) {{}}",
            );
            let item = parse_one(&source);
            assert!(
                missing_arguments_section(&item).is_empty(),
                "header `{header}` should suppress DOC004"
            );
        }
    }

    #[test]
    fn test_missing_arguments_rejects_unknown_header() {
        // A non-recognized header should still trigger DOC004.
        let item = parse_one(
            "/// Greets.\n///\n/// # Inputs\n///\n/// `name` - the name.\npub fn greet(name: &str) {}",
        );
        assert_eq!(
            missing_arguments_section(&item).len(),
            1,
            "`# Inputs` is not a recognized arguments header"
        );
    }

    #[test]
    fn test_missing_arguments_no_params() {
        let item = parse_one("/// Greets.\npub fn greet() {}");
        assert!(missing_arguments_section(&item).is_empty());
    }

    #[test]
    fn test_missing_arguments_private_skipped() {
        let item = parse_one("fn greet(name: &str) {}");
        assert!(missing_arguments_section(&item).is_empty());
    }

    // ── DOC005: undocumented_param ──

    #[test]
    fn test_undocumented_param_missing() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\npub fn build(name: &str, fmt: &str) {}",
        );
        let diags = undocumented_param(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_UNDOCUMENTED_PARAM);
        assert!(diags[0].message.contains("fmt"));
    }

    #[test]
    fn test_undocumented_param_all_documented() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\n/// `fmt` - the format.\npub fn build(name: &str, fmt: &str) {}",
        );
        assert!(undocumented_param(&item).is_empty());
    }

    #[test]
    fn test_undocumented_param_accepts_header_variants() {
        // DOC005 should detect documented params under any accepted header alias.
        for header in ["# Parameters", "# Params", "# Parameter", "# Param"] {
            let source = format!(
                "/// Builds.\n///\n/// {header}\n///\n/// `name` - the name.\n/// `fmt` - the format.\npub fn build(name: &str, fmt: &str) {{}}",
            );
            let item = parse_one(&source);
            assert!(
                undocumented_param(&item).is_empty(),
                "header `{header}` should be recognized by DOC005"
            );
        }
    }

    #[test]
    fn test_undocumented_param_no_section() {
        let item = parse_one("/// Builds.\npub fn build(name: &str) {}");
        assert!(undocumented_param(&item).is_empty());
    }

    // ── DOC006: doc_placeholder ──

    #[test]
    fn test_doc_placeholder_todo() {
        let item = parse_one("/// TODO: implement.\npub fn task() {}");
        let diags = doc_placeholder(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_DOC_PLACEHOLDER);
    }

    #[test]
    fn test_doc_placeholder_fixme() {
        let item = parse_one("/// FIXME: broken.\npub fn task() {}");
        assert_eq!(doc_placeholder(&item).len(), 1);
    }

    #[test]
    fn test_doc_placeholder_ellipsis() {
        let item = parse_one("/// Something ... here.\npub fn task() {}");
        assert_eq!(doc_placeholder(&item).len(), 1);
    }

    #[test]
    fn test_doc_placeholder_clean() {
        let item = parse_one("/// A clean doc.\npub fn task() {}");
        assert!(doc_placeholder(&item).is_empty());
    }

    #[test]
    fn test_doc_placeholder_non_documentable() {
        let item = parse_one("/// TODO.\nimpl Foo {}");
        assert!(doc_placeholder(&item).is_empty());
    }

    // ── TEST001: test_naming ──

    #[test]
    fn test_naming_test_prefix() {
        let item = parse_one("#[test]\nfn test_foo() {}");
        let diags = test_naming(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_TEST_NAMING);
    }

    #[test]
    fn test_naming_test_digits() {
        let item = parse_one("#[test]\nfn test1() {}");
        assert_eq!(test_naming(&item).len(), 1);
    }

    #[test]
    fn test_naming_case_prefix() {
        let item = parse_one("#[test]\nfn case_1() {}");
        assert_eq!(test_naming(&item).len(), 1);
    }

    #[test]
    fn test_naming_behavioral() {
        let item = parse_one("#[test]\nfn should_pass_when_valid() {}");
        assert!(test_naming(&item).is_empty());
    }

    #[test]
    fn test_naming_non_test() {
        let item = parse_one("fn helper() {}");
        assert!(test_naming(&item).is_empty());
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
