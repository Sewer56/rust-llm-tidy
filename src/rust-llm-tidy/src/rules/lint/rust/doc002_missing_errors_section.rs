//! `DOC002` - missing `# Errors` section on public `Result` functions.
//!
//! [`check`] fires on public functions returning `Result` that have no
//! `# Errors` header.

use super::{find_errors_section, is_pub_result_fn};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_ERRORS;
use crate::source::SourceItem;

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
        message: "pub fn returning Result is missing a `# Errors` doc section.\n\n\
                  Why: Readers need to understand possible failures and when they occur.\n\n\
                  Suggestions:\n\
                  - Add a `# Errors` section describing each error the implementation can return and its specific trigger.\n\
                  - If it cannot return an error, say so.\n\
                  - Do not invent errors or change behavior to satisfy this lint."
            .to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::rust::tests::parse_one;

    // ── DOC002: missing errors section ──

    // pub fn returns Result, no # Errors section -> error.
    #[test]
    fn check_should_request_actual_error_contract_when_section_is_missing() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");

        let diags = check(&item);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ERRORS);
        assert_eq!(
            diags[0].message,
            "pub fn returning Result is missing a `# Errors` doc section.\n\n\
             Why: Readers need to understand possible failures and when they occur.\n\n\
             Suggestions:\n\
             - Add a `# Errors` section describing each error the implementation can return and its specific trigger.\n\
             - If it cannot return an error, say so.\n\
             - Do not invent errors or change behavior to satisfy this lint."
        );
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
