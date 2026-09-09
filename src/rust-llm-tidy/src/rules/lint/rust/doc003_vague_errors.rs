//! `DOC003` - vague `# Errors` wording.
//!
//! [`check`] fires when an existing `# Errors` body names no concrete error
//! variant.

use super::{find_errors_section, is_pub_result_fn, section_body};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_VAGUE_ERRORS;
use crate::source::SourceItem;

/// `DOC003` - `# Errors` section must name concrete error variants.
///
/// Fires on `pub fn` returning `Result` when a `# Errors` section exists but
/// none of its bullets reference a concrete variant. A variant is detected
/// by the presence of a `::` path.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a vague `# Errors` section.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_result_fn(item) {
        return Vec::new();
    }
    let Some(start) = find_errors_section(item.doc_comments()) else {
        return Vec::new();
    };

    let body = section_body(item.doc_comments(), start);
    if section_names_variant(&body) {
        return Vec::new();
    }

    vec![Diagnostic {
        title: Some("vague `# Errors` section".into()),
        severity: Severity::Warning,
        code: CODE_VAGUE_ERRORS,
        message: "`# Errors` section has no `::` path naming a concrete error variant.\n\n\
                  Why: Concrete error names help readers connect failure conditions to handling code.\n\n\
                  Suggestions:\n\
                  - Document each error the implementation can return and its specific trigger.\n\
                  - Use variant paths where they exist.\n\
                  - If the error type has no variants or the function cannot fail, document that contract.\n\
                  - Do not invent variants or change behavior to silence this warning."
            .to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

/// True when any non-blank line in the section body references a concrete
/// variant via a path separator (`::`).
fn section_names_variant(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let t = line.trim();
        !t.is_empty() && t.contains("::")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::rust::tests::parse_one;

    // ── DOC003: vague errors ──

    // # Errors body names no concrete variant -> warning.
    #[test]
    fn check_should_request_actual_errors_when_section_has_no_variant_path() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns an error if loading fails.\npub fn load() -> Result<(), String> { Ok(()) }",
        );

        let diags = check(&item);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(
            diags[0].message,
            "`# Errors` section has no `::` path naming a concrete error variant.\n\n\
             Why: Concrete error names help readers connect failure conditions to handling code.\n\n\
             Suggestions:\n\
             - Document each error the implementation can return and its specific trigger.\n\
             - Use variant paths where they exist.\n\
             - If the error type has no variants or the function cannot fail, document that contract.\n\
             - Do not invent variants or change behavior to silence this warning."
        );
    }

    // # Errors section is empty (no body) -> still vague, warning.
    #[test]
    fn test_vague_errors_empty_section() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
    }

    // # Errors names a concrete variant (rustdoc link) -> no warning.
    #[test]
    fn test_vague_errors_with_variants() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns [Error::NotFound] if missing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(check(&item).is_empty());
    }

    // No # Errors section at all -> not applicable, skipped.
    #[test]
    fn test_vague_errors_no_section_skipped() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        assert!(check(&item).is_empty());
    }

    // Generic markdown link with no `::` path is not a concrete variant -> warning.
    #[test]
    fn test_vague_errors_generic_link() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// See [the configuration guide].\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
    }
}
