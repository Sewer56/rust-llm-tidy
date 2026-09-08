//! `DOC001` - missing doc comments on non-private items.
//!
//! [`check`] fires on `pub` and `pub(crate)`/`pub(super)`/`pub(in path)`
//! items of documentable kinds that carry no leading `///` doc comment. Test
//! modules are skipped.

use super::is_documentable;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_DOCS;
use crate::source::{SourceItem, VisibilityTier};

/// `DOC001` - non-private documentable items must have a `///` doc comment.
///
/// Fires on `pub` and `pub(crate)`/`pub(super)`/`pub(in path)` items of
/// documentable kinds (fn, struct, enum, ...) that have zero leading doc
/// comments. Test modules are skipped.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a missing `///` doc comment
///   on a non-private documentable item.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
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
        message: "non-private item is missing a doc comment.\n\n\
                  Why: Readers need its purpose and contract without tracing the implementation.\n\n\
                  Suggestions:\n\
                  - Add `///` docs describing its \
                  purpose and actual contract, based on the implementation and relevant callers."
            .to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::rust::tests::parse_one;

    // ── DOC001: missing docs ──

    // Public function with no doc comment -> reports an error.
    #[test]
    fn check_should_explain_required_docs_when_public_item_is_undocumented() {
        let item = parse_one("pub fn do_thing() {}");

        let diags = check(&item);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_DOCS);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(
            diags[0].message,
            "non-private item is missing a doc comment.\n\n\
             Why: Readers need its purpose and contract without tracing the implementation.\n\n\
             Suggestions:\n\
             - Add `///` docs describing its \
             purpose and actual contract, based on the implementation and relevant callers."
        );
    }

    // Has a doc comment -> no error.
    #[test]
    fn test_missing_docs_documented() {
        let item = parse_one("/// Does the thing.\npub fn do_thing() {}");
        assert!(check(&item).is_empty());
    }

    // Private item -> skipped, no error.
    #[test]
    fn test_missing_docs_private_skipped() {
        let item = parse_one("fn helper() {}");
        assert!(check(&item).is_empty());
    }

    // Public struct with no doc comment -> reports an error.
    #[test]
    fn test_missing_docs_pub_struct() {
        let item = parse_one("pub struct Foo;");
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
    }

    // pub(crate) item with no doc comment -> reports an error.
    #[test]
    fn test_missing_docs_pub_crate() {
        let item = parse_one("pub(crate) fn internal() {}");
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
    }

    // Test module -> skipped, no error.
    #[test]
    fn test_missing_docs_test_mod_skipped() {
        let source = "#[cfg(test)]\npub mod tests {}";
        let item = parse_one(source);
        assert!(check(&item).is_empty());
    }

    // use statement -> not documentable, skipped.
    #[test]
    fn test_missing_docs_use_skipped() {
        let item = parse_one("pub use std::io;");
        assert!(check(&item).is_empty());
    }

    // Item starts on line 3 (after two doc lines); the reported diagnostic
    // line must equal the precomputed start line, not 1.
    #[test]
    fn test_start_line_is_reported() {
        let item = parse_one("\n\npub fn do_thing() {}");
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }
}
