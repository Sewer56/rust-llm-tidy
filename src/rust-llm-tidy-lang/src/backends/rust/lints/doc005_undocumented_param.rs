//! `DOC005` - undocumented parameters in an existing `# Arguments` section.
//!
//! [`check`] fires when an existing section body omits at least one
//! parameter name.

use super::{contains_word, find_arguments_section, is_pub_fn_with_params, section_body};
use rust_llm_tidy_lint::check::CODE_UNDOCUMENTED_PARAM;
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::SourceItem;

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
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::rust::lints::tests::parse_one;

    // ── DOC005: undocumented param ──

    // One param (fmt) missing from # Arguments -> warning.
    #[test]
    fn test_undocumented_param_missing() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\npub fn build(name: &str, fmt: &str) {}",
        );
        let diags = check(&item);
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
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }

    // All params documented -> no warning.
    #[test]
    fn test_undocumented_param_all_documented() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\n/// `fmt` - the format.\npub fn build(name: &str, fmt: &str) {}",
        );
        assert!(check(&item).is_empty());
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
                check(&item).is_empty(),
                "header `{header}` should be recognized by DOC005"
            );
        }
    }

    // No # Arguments section at all -> not applicable, skipped.
    #[test]
    fn test_undocumented_param_no_section() {
        let item = parse_one("/// Builds.\npub fn build(name: &str) {}");
        assert!(check(&item).is_empty());
    }
}
