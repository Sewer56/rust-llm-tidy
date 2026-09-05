//! `DOC004` - missing `<param>` doc tags on parameterized members.

use super::Declaration;
use rust_llm_tidy_lint::check::CODE_MISSING_ARGUMENTS;
use rust_llm_tidy_lint::{Diagnostic, Severity};

/// `DOC004` - members with parameters need `<param name="...">` tags.
///
/// Fires on non-private methods, constructors, and indexers that declare
/// parameters and whose docs carry no `<param>` tag.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((_, tags)) = &decl.param_scan else {
        return Vec::new();
    };
    if !tags.is_empty() {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Warning,
        CODE_MISSING_ARGUMENTS,
        "member with parameters is missing `<param>` doc tags".to_string(),
    )]
}
