//! `DOC001` - missing doc comments on non-private declarations.

use super::{DOCUMENTABLE, Declaration};
use rust_llm_tidy_lint::check::CODE_MISSING_DOCS;
use rust_llm_tidy_lint::{Diagnostic, Severity};

/// `DOC001` - non-private documentable declarations need a `///` doc
/// comment.
///
/// Fires on `public`, `internal`, and `protected`-family declarations of
/// documentable kinds that carry no `///` doc comment.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    if !decl.non_private || !DOCUMENTABLE.contains(&decl.kind) || !decl.docs.is_empty() {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Error,
        CODE_MISSING_DOCS,
        "non-private item is missing a doc comment".to_string(),
    )]
}
