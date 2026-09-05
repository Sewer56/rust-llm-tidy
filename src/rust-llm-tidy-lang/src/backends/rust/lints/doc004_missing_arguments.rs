//! `DOC004` - missing `# Arguments` section on public functions with
//! parameters.
//!
//! [`check`] fires on public functions with parameters that have no
//! `# Arguments` header.

use super::{find_arguments_section, is_pub_fn_with_params};
use rust_llm_tidy_lint::check::CODE_MISSING_ARGUMENTS;
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::SourceItem;

/// `DOC004` - `pub fn` with parameters must have an `# Arguments` section.
///
/// Fires on fully-public functions (`pub fn`) that declare at least one named
/// parameter (excluding `self`) and whose doc comments contain no `# Arguments`
/// or `# Parameters` header.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a missing `# Arguments`
///   section on a `pub fn` with parameters.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_fn_with_params(item) {
        return Vec::new();
    }
    if find_arguments_section(item.doc_comments()).is_some() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_MISSING_ARGUMENTS,
        message: "pub fn with parameters is missing a `# Arguments` doc section".to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::rust::lints::tests::parse_one;

    // ── DOC004: missing arguments section ──

    // pub fn with params, no # Arguments section -> warning.
    #[test]
    fn test_missing_arguments_no_section() {
        let item = parse_one("/// Greets.\npub fn greet(name: &str) {}");
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ARGUMENTS);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    // Has an # Arguments section -> no warning.
    #[test]
    fn test_missing_arguments_has_section() {
        let item = parse_one(
            "/// Greets.\n///\n/// # Arguments\n///\n/// `name` - the name.\npub fn greet(name: &str) {}",
        );
        assert!(check(&item).is_empty());
    }

    // Any recognized header alias suppresses the warning.
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
                check(&item).is_empty(),
                "header `{header}` should suppress DOC004"
            );
        }
    }

    // Unrecognized header (# Inputs) still triggers the warning.
    #[test]
    fn test_missing_arguments_rejects_unknown_header() {
        // A non-recognized header should still trigger DOC004.
        let item = parse_one(
            "/// Greets.\n///\n/// # Inputs\n///\n/// `name` - the name.\npub fn greet(name: &str) {}",
        );
        assert_eq!(
            check(&item).len(),
            1,
            "`# Inputs` is not a recognized arguments header"
        );
    }

    // No parameters -> not applicable, no warning.
    #[test]
    fn test_missing_arguments_no_params() {
        let item = parse_one("/// Greets.\npub fn greet() {}");
        assert!(check(&item).is_empty());
    }

    // Private fn -> skipped, no warning.
    #[test]
    fn test_missing_arguments_private_skipped() {
        let item = parse_one("fn greet(name: &str) {}");
        assert!(check(&item).is_empty());
    }
}
