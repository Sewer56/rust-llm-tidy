//! `DOC002` - missing `<exception>` doc tag on throwing members.

use super::Declaration;
use rust_llm_tidy_lint::check::CODE_MISSING_ERRORS;
use rust_llm_tidy_lint::{Diagnostic, Severity};

/// `DOC002` - members that throw need an `<exception>` doc tag.
///
/// Fires on non-private methods and constructors whose body holds a
/// `throw` statement and whose docs carry no `<exception>` tag. The
/// heuristic body scan can miss rethrows through helpers.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((tag_count, _)) = &decl.exception_scan else {
        return Vec::new();
    };
    if *tag_count != 0 {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Error,
        CODE_MISSING_ERRORS,
        "member that throws is missing an `<exception>` doc tag".to_string(),
    )]
}
