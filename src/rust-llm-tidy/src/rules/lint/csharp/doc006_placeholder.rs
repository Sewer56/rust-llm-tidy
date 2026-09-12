//! `DOC006` - placeholder markers in doc comments.

use super::{DOCUMENTABLE, Declaration};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_DOC_PLACEHOLDER;

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

    vec![
        decl.diagnostic(
            Severity::Warning,
            CODE_DOC_PLACEHOLDER,
            "placeholder text",
            "doc comment contains placeholder text (TODO/FIXME/TBD).\n\n\
         Why: Placeholders leave readers without an explanation of current behavior.\n\n\
         Suggestions:\n\
         - Replace the placeholder with an accurate description of the existing contract."
                .to_string(),
        ),
    ]
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric character other than `_` (or
/// the start/end of the text). This mirrors the Rust rules' matcher: the
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    #[test]
    fn diagnostic_should_request_accurate_contract_instead_of_placeholder() {
        let parsed = parse("/// <summary>TODO</summary>\npublic class Cache {}").unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_DOC_PLACEHOLDER)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            "doc comment contains placeholder text (TODO/FIXME/TBD).\n\n\
             Why: Placeholders leave readers without an explanation of current behavior.\n\n\
             Suggestions:\n\
             - Replace the placeholder with an accurate description of the existing contract."
        );
    }
}
