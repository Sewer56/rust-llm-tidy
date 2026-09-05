//! `DOC006` - placeholder text in doc comments.
//!
//! [`check`] fires on documentable items whose doc comments contain a
//! placeholder marker (`TODO`, `FIXME`, or `TBD`).

use super::{contains_word, is_documentable};
use rust_llm_tidy_lint::check::CODE_DOC_PLACEHOLDER;
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::SourceItem;

/// `DOC006` - doc comments must not contain placeholder text.
///
/// Fires on documentable items whose doc comments contain a placeholder marker
/// (`TODO`, `FIXME`, or `TBD`). Such markers signal unfinished docs that read as
/// finished API documentation.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for placeholder text in its doc
///   comments.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if !is_documentable(item.kind()) {
        return Vec::new();
    }
    let docs = item.doc_comments();
    if docs.is_empty() {
        return Vec::new();
    }
    if !docs.iter().any(|d| contains_placeholder(d)) {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_DOC_PLACEHOLDER,
        message: "doc comment contains placeholder text (TODO/FIXME/TBD)".to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

fn contains_placeholder(text: &str) -> bool {
    contains_word(text, "todo") || contains_word(text, "fixme") || contains_word(text, "tbd")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::rust::lints::tests::parse_one;

    // ── DOC006: doc placeholder ──

    // TODO marker in doc -> warning.
    #[test]
    fn test_doc_placeholder_todo() {
        let item = parse_one("/// TODO: implement.\npub fn task() {}");
        let diags = check(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_DOC_PLACEHOLDER);
    }

    // FIXME marker in doc -> warning.
    #[test]
    fn test_doc_placeholder_fixme() {
        let item = parse_one("/// FIXME: broken.\npub fn task() {}");
        assert_eq!(check(&item).len(), 1);
    }

    // `...` is unambiguous (ellipsis) and idiomatic in prose, so it is NOT
    // treated as a placeholder marker. Rust shorthand like `Result<...>` is
    // unaffected.
    #[test]
    fn test_doc_placeholder_ellipsis_not_flagged() {
        let item = parse_one("/// Something ... here.\npub fn task() {}");
        assert!(check(&item).is_empty());
    }

    // Clean doc, no placeholder -> no warning.
    #[test]
    fn test_doc_placeholder_clean() {
        let item = parse_one("/// A clean doc.\npub fn task() {}");
        assert!(check(&item).is_empty());
    }

    // todo inside non-documentable item (impl) -> skipped.
    #[test]
    fn test_doc_placeholder_non_documentable() {
        let item = parse_one("/// TODO.\nimpl Foo {}");
        assert!(check(&item).is_empty());
    }
}
