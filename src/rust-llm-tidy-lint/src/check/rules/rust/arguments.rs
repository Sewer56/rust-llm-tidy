//! `DOC004` - missing `# Arguments` section, and `DOC005` - undocumented params.
//!
//! [`missing_arguments_section`] fires on public functions with parameters that
//! have no `# Arguments` header.
//!
//! [`undocumented_param`] fires when an existing section body omits at least
//! one parameter name.
//!
//! Both share [`ARGUMENTS_HEADERS`], [`find_arguments_section`], and
//! [`is_pub_fn_with_params`].

use super::placeholder::contains_word;
use super::section_body;
use crate::check::{CODE_MISSING_ARGUMENTS, CODE_UNDOCUMENTED_PARAM};
use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{SourceItem, VisibilityTier};

/// Accepted rustdoc headers for documenting function parameters.
///
/// All variants are matched case-insensitively, so `# Arguments`, `# arguments`,
/// and `# ARGUMENTS` are equivalent.
const ARGUMENTS_HEADERS: &[&str] = &[
    "# Arguments",
    "# Argument",
    "# Parameters",
    "# Parameter",
    "# Params",
    "# Param",
];

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
pub fn missing_arguments_section(item: &SourceItem) -> Vec<Diagnostic> {
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

/// `DOC005` - `# Arguments` section must mention every parameter name.
///
/// Fires on `pub fn` with parameters when an `# Arguments`/`# Parameters`
/// section exists but at least one parameter name is not mentioned anywhere in
/// the section body.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for undocumented parameters in
///   its `# Arguments`/`# Parameters` section.
pub fn undocumented_param(item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_fn_with_params(item) {
        return Vec::new();
    }
    let Some(start) = find_arguments_section(item.doc_comments()) else {
        return Vec::new();
    };

    let body = section_body(item.doc_comments(), start);
    let undocumented: Vec<&str> = item
        .params()
        .iter()
        .filter(|p| !body.iter().any(|line| contains_word(line, p.as_str())))
        .map(String::as_str)
        .collect();

    if undocumented.is_empty() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_UNDOCUMENTED_PARAM,
        message: format!(
            "parameter(s) not documented in the `# Arguments` section: `{}`",
            undocumented.join("`, `")
        ),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

/// Index into `doc_comments` of a parameter-documentation header, if present.
///
/// Accepts the common rustdoc variants `# Arguments`, `# Parameters`, and
/// `# Params` (plus their singulars), matched case-insensitively.
fn find_arguments_section(docs: &[String]) -> Option<usize> {
    docs.iter().position(|d| {
        let t = d.trim().to_ascii_lowercase();
        ARGUMENTS_HEADERS
            .iter()
            .any(|h| t == h.to_ascii_lowercase())
    })
}

/// True when `item` is a `pub fn` that declares at least one named parameter
/// (the `self` receiver does not count).
fn is_pub_fn_with_params(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && !item.params().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::tests::parse_one;

    // ── DOC004: missing_arguments_section ──

    // pub fn with params, no # Arguments section -> warning.
    #[test]
    fn test_missing_arguments_no_section() {
        let item = parse_one("/// Greets.\npub fn greet(name: &str) {}");
        let diags = missing_arguments_section(&item);
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
        assert!(missing_arguments_section(&item).is_empty());
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
                missing_arguments_section(&item).is_empty(),
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
            missing_arguments_section(&item).len(),
            1,
            "`# Inputs` is not a recognized arguments header"
        );
    }

    // No parameters -> not applicable, no warning.
    #[test]
    fn test_missing_arguments_no_params() {
        let item = parse_one("/// Greets.\npub fn greet() {}");
        assert!(missing_arguments_section(&item).is_empty());
    }

    // Private fn -> skipped, no warning.
    #[test]
    fn test_missing_arguments_private_skipped() {
        let item = parse_one("fn greet(name: &str) {}");
        assert!(missing_arguments_section(&item).is_empty());
    }

    // ── DOC005: undocumented_param ──

    // One param (fmt) missing from # Arguments -> warning.
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

    // Param name that only appears as a substring (name in filename) -> warning.
    #[test]
    fn test_undocumented_param_whole_word() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `filename` - the file.\npub fn build(name: &str) {}",
        );
        let diags = undocumented_param(&item);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }

    // All params documented -> no warning.
    #[test]
    fn test_undocumented_param_all_documented() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\n/// `fmt` - the format.\npub fn build(name: &str, fmt: &str) {}",
        );
        assert!(undocumented_param(&item).is_empty());
    }

    // Params documented under any recognized header alias -> no warning.
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

    // No # Arguments section at all -> not applicable, skipped.
    #[test]
    fn test_undocumented_param_no_section() {
        let item = parse_one("/// Builds.\npub fn build(name: &str) {}");
        assert!(undocumented_param(&item).is_empty());
    }
}
