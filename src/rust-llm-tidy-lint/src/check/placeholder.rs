//! `DOC006` - placeholder text in doc comments.
//!
//! [`doc_placeholder`] fires on documentable items whose doc comments contain a
//! placeholder marker (`TODO`, `FIXME`, `TBD`, or `...`). Detection is
//! delegated to the module-private [`contains_placeholder`] and the
//! crate-visible [`contains_word`] helper.

use crate::check::CODE_DOC_PLACEHOLDER;
use crate::check::is_documentable;
use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::SourceItem;

/// `DOC006` - doc comments must not contain placeholder text.
///
/// Fires on documentable items whose doc comments contain a placeholder marker
/// (`TODO`, `FIXME`, `TBD`, or `...`). Such markers signal unfinished docs that
/// read as finished API documentation.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for placeholder text in its doc
///   comments.
pub fn doc_placeholder(item: &SourceItem) -> Vec<Diagnostic> {
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
        message: "doc comment contains placeholder text (TODO/FIXME/TBD/...)".to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric, non-underscore character (or the
/// start/end of the text), so `todo` matches in `// TODO:` but not in
/// `todolist`, and `name` matches in `` `name` `` but not in `filename`.
///
/// Used by DOC006 ([`contains_placeholder`]) and DOC005 in `check::arguments`.
pub(crate) fn contains_word(haystack: &str, needle: &str) -> bool {
    let h = haystack.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    let mut start = 0;
    while let Some(pos) = h[start..].find(&n) {
        let abs = start + pos;
        let before_ok = h[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after_idx = abs + n.len();
        let after_ok = h[after_idx..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if before_ok && after_ok {
            return true;
        }
        start = abs + n.len();
    }
    false
}

/// True when `text` contains a placeholder marker: a whole-word `TODO`,
/// `FIXME`, or `TBD` (case-insensitive), or a literal `...`.
fn contains_placeholder(text: &str) -> bool {
    contains_word(text, "todo")
        || contains_word(text, "fixme")
        || contains_word(text, "tbd")
        || text.contains("...")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::tests::parse_one;

    // ── DOC006: doc_placeholder ──

    // TODO marker in doc -> warning.
    #[test]
    fn test_doc_placeholder_todo() {
        let item = parse_one("/// TODO: implement.\npub fn task() {}");
        let diags = doc_placeholder(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_DOC_PLACEHOLDER);
    }

    // FIXME marker in doc -> warning.
    #[test]
    fn test_doc_placeholder_fixme() {
        let item = parse_one("/// FIXME: broken.\npub fn task() {}");
        assert_eq!(doc_placeholder(&item).len(), 1);
    }

    // Ellipsis (...) in doc -> warning.
    #[test]
    fn test_doc_placeholder_ellipsis() {
        let item = parse_one("/// Something ... here.\npub fn task() {}");
        assert_eq!(doc_placeholder(&item).len(), 1);
    }

    // Clean doc, no placeholder -> no warning.
    #[test]
    fn test_doc_placeholder_clean() {
        let item = parse_one("/// A clean doc.\npub fn task() {}");
        assert!(doc_placeholder(&item).is_empty());
    }

    // todo inside non-documentable item (impl) -> skipped.
    #[test]
    fn test_doc_placeholder_non_documentable() {
        let item = parse_one("/// TODO.\nimpl Foo {}");
        assert!(doc_placeholder(&item).is_empty());
    }
}
