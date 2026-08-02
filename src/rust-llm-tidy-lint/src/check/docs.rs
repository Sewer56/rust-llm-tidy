//! `DOC001` - missing doc comments on non-private items.
//!
//! [`missing_docs`] fires on `pub` and `pub(crate)`/`pub(super)`/`pub(in path)`
//! items of documentable kinds that carry no leading `///` doc comment. Test
//! modules are skipped.

use crate::check::shared::is_documentable;
use crate::check::CODE_MISSING_DOCS;
use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{SourceItem, VisibilityTier};

/// `DOC001` - non-private documentable items must have a `///` doc comment.
///
/// Fires on `pub` and `pub(crate)`/`pub(super)`/`pub(in path)` items of
/// documentable kinds (fn, struct, enum, ...) that have zero leading doc
/// comments. Test modules are skipped.
pub fn missing_docs(item: &SourceItem) -> Vec<Diagnostic> {
    let Some(vis) = item.visibility() else {
        return Vec::new();
    };
    if vis == VisibilityTier::Private {
        return Vec::new();
    }
    if !is_documentable(item.kind()) {
        return Vec::new();
    }
    if item.is_test_module() {
        return Vec::new();
    }
    if !item.doc_comments().is_empty() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Error,
        code: CODE_MISSING_DOCS,
        message: "non-private item is missing a doc comment".to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}
