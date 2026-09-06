//! `DOC005` - `<param>` tags that omit declared parameters.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_UNDOCUMENTED_PARAM;

/// `DOC005` - `<param>` tags must name every declared parameter.
///
/// Fires on parameterized members whose `<param>` tags omit at least one
/// declared parameter name.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((params, tags)) = &decl.param_scan else {
        return Vec::new();
    };
    if tags.is_empty() {
        return Vec::new();
    }
    let missing: Vec<&str> = params
        .iter()
        .map(String::as_str)
        .filter(|p| !tags.iter().any(|tag| tag == p))
        .collect();
    if missing.is_empty() {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Warning,
        CODE_UNDOCUMENTED_PARAM,
        format!(
            "parameter(s) not documented in `<param>` tags: `{}`",
            missing.join("`, `")
        ),
    )]
}
