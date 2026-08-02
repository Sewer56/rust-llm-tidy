//! `DOC002` - missing `# Errors` section, and `DOC003` - vague errors wording.
//!
//! [`missing_errors_section`] fires on public functions returning `Result` that
//! have no `# Errors` header; [`vague_errors`] fires when an existing `# Errors`
//! body names no concrete error variant. Both share [`is_pub_result_fn`],
//! [`find_errors_section`], and [`section_names_variant`].

use crate::check::shared::section_body;
use crate::check::{CODE_MISSING_ERRORS, CODE_VAGUE_ERRORS};
use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{SourceItem, VisibilityTier};

/// `DOC002` - `pub fn` returning `Result` must have a `# Errors` section.
///
/// Fires on fully-public functions (`pub fn`) whose return type ends in
/// `Result` and whose doc comments contain no `# Errors` header.
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
/// of a rustdoc link `[...]` or a `::` path).
pub fn vague_errors(item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_result_fn(item) {
        return Vec::new();
    }
    let Some(start) = find_errors_section(item.doc_comments()) else {
        return Vec::new();
    };

    let body = section_body(item.doc_comments(), start);
    if body.is_empty() {
        return Vec::new();
    }
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

/// True when `item` is a `pub fn` whose return type ends in `Result`.
fn is_pub_result_fn(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && item.returns_result()
}

/// Index into `doc_comments` of the `# Errors` section header, if present.
fn find_errors_section(docs: &[String]) -> Option<usize> {
    docs.iter().position(|d| d.trim() == "# Errors")
}

/// True when any non-blank line in the section body references a concrete
/// variant via a rustdoc link (`[`) or path separator (`::`).
fn section_names_variant(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let t = line.trim();
        !t.is_empty() && (t.contains('[') || t.contains("::"))
    })
}
