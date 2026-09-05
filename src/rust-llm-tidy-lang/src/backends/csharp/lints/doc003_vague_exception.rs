//! `DOC003` - `<exception>` tags naming no concrete exception type.

use super::Declaration;
use rust_llm_tidy_lint::check::CODE_VAGUE_ERRORS;
use rust_llm_tidy_lint::{Diagnostic, Severity};

/// `DOC003` - members that can throw need `<exception>` tags with a
/// concrete `cref` type.
///
/// Fires on non-private members that can throw, directly or through
/// same-file calls, whose `<exception>` tags all lack a concrete
/// `cref` value.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((tag_count, crefs)) = &decl.exception_scan else {
        return Vec::new();
    };
    if *tag_count == 0 || crefs.iter().any(|cref| !cref.trim().is_empty()) {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Warning,
        CODE_VAGUE_ERRORS,
        "`<exception>` doc tags name no concrete exception type (`cref`)".to_string(),
    )]
}
