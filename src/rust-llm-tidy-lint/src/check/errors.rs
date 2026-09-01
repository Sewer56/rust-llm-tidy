//! `DOC002` - missing `# Errors` section, and `DOC003` - vague errors wording.
//!
//! [`missing_errors_section`] fires on public functions returning `Result` that
//! have no `# Errors` header.
//!
//! [`vague_errors`] fires when an existing `# Errors` body names no concrete
//! error variant.
//!
//! Both share [`is_pub_result_fn`], [`find_errors_section`], and
//! [`section_names_variant`].

use crate::check::section_body;
use crate::check::{CODE_MISSING_ERRORS, CODE_VAGUE_ERRORS};
use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{SourceItem, VisibilityTier};

/// `DOC002` - `pub fn` returning `Result` must have a `# Errors` section.
///
/// Fires on fully-public functions (`pub fn`) whose return type ends in
/// `Result` and whose doc comments contain no `# Errors` header.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a missing `# Errors`
///   section on a `pub fn` returning `Result`.
pub fn missing_errors_section(item: &SourceItem) -> Vec<Diagnostic> {
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

/// `DOC003` - `# Errors` section must name concrete error variants.
///
/// Fires on `pub fn` returning `Result` when a `# Errors` section exists but
/// none of its bullets reference a concrete variant (detected by the presence
/// of a `::` path).
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a vague `# Errors` section.
pub fn vague_errors(item: &SourceItem) -> Vec<Diagnostic> {
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
        severity: Severity::Warning,
        code: CODE_VAGUE_ERRORS,
        message: "`# Errors` section does not name any concrete error variant".to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

/// Index into `doc_comments` of the `# Errors` section header, if present.
fn find_errors_section(docs: &[String]) -> Option<usize> {
    docs.iter()
        .position(|d| d.trim().eq_ignore_ascii_case("# errors"))
}

/// True when `item` is a `pub fn` whose return type ends in `Result`.
fn is_pub_result_fn(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && item.returns_result()
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
    use crate::check::tests::parse_one;

    // ── DOC002: missing_errors_section ──

    // pub fn returns Result, no # Errors section -> error.
    #[test]
    fn test_missing_errors_no_section() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        let diags = missing_errors_section(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ERRORS);
    }

    // Has an # Errors section -> no error.
    #[test]
    fn test_missing_errors_has_section() {
        let item = parse_one(
            "/// Loads a file.\n///\n/// # Errors\n///\n/// Returns nothing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(missing_errors_section(&item).is_empty());
    }

    // Lowercase # errors header is still recognized -> no error.
    #[test]
    fn test_missing_errors_lowercase_header() {
        let item = parse_one(
            "/// Loads a file.\n///\n/// # errors\n///\n/// Returns nothing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(missing_errors_section(&item).is_empty());
    }

    // Does not return Result -> not applicable, no error.
    #[test]
    fn test_missing_errors_not_result() {
        let item = parse_one("pub fn load() -> u32 { 0 }");
        assert!(missing_errors_section(&item).is_empty());
    }

    // Private fn -> skipped, no error.
    #[test]
    fn test_missing_errors_private_skipped() {
        let item = parse_one("fn load() -> Result<(), String> { Ok(()) }");
        assert!(missing_errors_section(&item).is_empty());
    }

    // ── DOC003: vague_errors ──

    // # Errors body names no concrete variant -> warning.
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

    // # Errors section is empty (no body) -> still vague, warning.
    #[test]
    fn test_vague_errors_empty_section() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        let diags = vague_errors(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
    }

    // # Errors names a concrete variant (rustdoc link) -> no warning.
    #[test]
    fn test_vague_errors_with_variants() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns [Error::NotFound] if missing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(vague_errors(&item).is_empty());
    }

    // No # Errors section at all -> not applicable, skipped.
    #[test]
    fn test_vague_errors_no_section_skipped() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        assert!(vague_errors(&item).is_empty());
    }

    // Generic markdown link with no `::` path is not a concrete variant -> warning.
    #[test]
    fn test_vague_errors_generic_link() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// See [the configuration guide].\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        let diags = vague_errors(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
    }
}
