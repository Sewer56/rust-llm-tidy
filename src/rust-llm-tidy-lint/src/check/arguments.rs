//! `DOC004` - missing `# Arguments` section, and `DOC005` - undocumented params.
//!
//! [`missing_arguments_section`] fires on public functions with parameters that
//! have no `# Arguments` header; [`undocumented_param`] fires when an existing
//! section body omits at least one parameter name. Both share
//! [`ARGUMENTS_HEADERS`], [`find_arguments_section`], and
//! [`is_pub_fn_with_params`].

use crate::check::shared::section_body;
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
        .filter(|p| !body.iter().any(|line| line.contains(p.as_str())))
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
