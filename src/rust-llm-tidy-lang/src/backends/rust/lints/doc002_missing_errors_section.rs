//! `DOC002` - missing `# Errors` section on public `Result` functions.
//!
//! [`check`] fires on public functions returning `Result` that have no
//! `# Errors` header.

use super::{find_errors_section, is_pub_result_fn};
use rust_llm_tidy_lint::check::CODE_MISSING_ERRORS;
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::SourceItem;

/// `DOC002` - `pub fn` returning `Result` must have an `# Errors` section.
///
/// Fires on fully-public functions (`pub fn`) whose return type ends in
/// `Result` and whose doc comments contain no `# Errors` header.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a missing `# Errors`
///   section on a `pub fn` returning `Result`.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_result_fn(item) {
        return Vec::new();
    }
    if find_errors_section(item.doc_comments()).is_some() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Error,
        code: CODE_MISSING_ERRORS,
        message: "pub fn returning Result is missing a `# Errors` doc section".to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::rust::lints::tests::parse_one;

    // ── DOC002: missing errors section ──

    // pub fn returns Result, no # Errors section -> error.
    #[test]
    fn test_missing_errors_no_section() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ERRORS);
    }

    // Has an # Errors section -> no error.
    #[test]
    fn test_missing_errors_has_section() {
        let item = parse_one(
            "/// Loads a file.\n///\n/// # Errors\n///\n/// Returns nothing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(check(&item).is_empty());
    }

    // Lowercase # errors header is still recognized -> no error.
    #[test]
    fn test_missing_errors_lowercase_header() {
        let item = parse_one(
            "/// Loads a file.\n///\n/// # errors\n///\n/// Returns nothing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(check(&item).is_empty());
    }

    // Does not return Result -> not applicable, no error.
    #[test]
    fn test_missing_errors_not_result() {
        let item = parse_one("pub fn load() -> u32 { 0 }");
        assert!(check(&item).is_empty());
    }

    // Private fn -> skipped, no error.
    #[test]
    fn test_missing_errors_private_skipped() {
        let item = parse_one("fn load() -> Result<(), String> { Ok(()) }");
        assert!(check(&item).is_empty());
    }
}
