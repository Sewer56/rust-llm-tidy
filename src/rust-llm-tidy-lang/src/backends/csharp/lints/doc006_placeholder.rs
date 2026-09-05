//! `DOC006` - placeholder markers in doc comments.

use super::{DOCUMENTABLE, Declaration};
use rust_llm_tidy_lint::check::CODE_DOC_PLACEHOLDER;
use rust_llm_tidy_lint::{Diagnostic, Severity};

/// The placeholder markers DOC006 scans for, lowercase.
const MARKERS: &[&str] = &["todo", "fixme", "tbd"];

/// `DOC006` - doc comments must not contain placeholder text.
///
/// Fires on documentable declarations whose doc comments contain a
/// placeholder marker word; see [`MARKERS`] for the accepted set.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    if !DOCUMENTABLE.contains(&decl.kind)
        || !decl
            .docs
            .iter()
            .any(|doc| MARKERS.iter().any(|m| contains_word(doc, m)))
    {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Warning,
        CODE_DOC_PLACEHOLDER,
        "doc comment contains placeholder text (TODO/FIXME/TBD)".to_string(),
    )]
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric, non-underscore character (or
/// the start/end of the text), mirroring the Rust rules' matcher: the
/// needle matches when framed by punctuation but never inside a longer
/// word.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let lower = haystack.to_ascii_lowercase();
    let mut start = 0;
    while let Some(pos) = lower[start..].find(needle) {
        let abs = start + pos;
        let before_ok = lower[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let after = abs + needle.len();
        let after_ok = lower[after..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if before_ok && after_ok {
            return true;
        }
        start = abs + needle.len();
    }
    false
}
