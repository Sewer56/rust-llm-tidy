//! `DOC002` - missing `<exception>` doc tag on members that can throw.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_ERRORS;

/// `DOC002` - members that can throw need an `<exception>` doc tag.
///
/// Fires on non-private methods and constructors that can throw and
/// whose docs carry no `<exception>` tag.
///
/// Throwing evidence is recursive: a `throw` in the member's own body
/// or a call to a same-file member that can throw, transitively.
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
