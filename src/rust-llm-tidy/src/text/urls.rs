//! Recognize URL tokens in measured prose.
//!
//! Scheme matching is an explicit allowlist, not a generic `scheme:` scan,
//! so ordinary prose is never mistaken for a URL. A scheme must sit on a
//! token boundary and carry a nonempty destination.

use core::ops::Range;

/// URL schemes recognized in prose, matched case-insensitively as a prefix.
const SCHEME_PREFIXES: &[&str] = &[
    "http://", "https://", "ftp://", "ftps://", "ssh://", "git://", "ws://", "wss://", "file://",
    "mailto:", "nxm://", "r2:",
];

/// The byte range of a URL that ends `line`, ignoring trailing sentence
/// punctuation and markdown closers.
///
/// The range covers the URL token alone: surrounding markdown, link labels,
/// and trailing punctuation stay outside it. Returns `None` when the line
/// does not end with a recognized URL.
pub(crate) fn trailing_range(line: &str) -> Option<Range<usize>> {
    for (start, _) in line.char_indices() {
        let Some(scheme) = scheme_at(line, start) else {
            continue;
        };

        // The URL body ends at whitespace or a markdown closer; the tail
        // then trims back to the URL proper, dropping sentence punctuation.
        let body_end = url_body_end(line, start + scheme.len());
        let url = trim_tail(&line[start..body_end]);
        if url.len() <= scheme.len() {
            continue;
        }

        let url_end = start + url.len();
        if line[url_end..].chars().all(is_ignorable_tail) {
            return Some(start..url_end);
        }
    }
    None
}

/// The recognized scheme starting at byte `index`, if any.
///
/// Returns `None` when `index` is mid-token or no listed scheme matches.
pub(crate) fn scheme_at(text: &str, index: usize) -> Option<&'static str> {
    if !starts_token(text, index) {
        return None;
    }
    let tail = &text[index..];
    SCHEME_PREFIXES
        .iter()
        .copied()
        .find(|scheme| starts_with_ignore_ascii_case(tail, scheme))
}

/// Whether `ch` may trail a URL without ending its exemption: sentence
/// punctuation, markdown closers, or whitespace.
fn is_ignorable_tail(ch: char) -> bool {
    ch.is_whitespace() || is_sentence_punctuation(ch) || matches!(ch, '>' | ']' | ')')
}

/// Whether `text` starts a token at `index`, not the middle of a word.
///
/// Rejects a scheme glued to an alphanumeric run or scheme punctuation, so
/// `xhttps://` and `git+https://` never read as a URL.
fn starts_token(text: &str, index: usize) -> bool {
    match text[..index].chars().next_back() {
        None => true,
        Some(previous) => {
            !previous.is_alphanumeric() && !matches!(previous, '+' | '.' | '-' | '_' | '/')
        }
    }
}

/// Case-insensitive ASCII prefix test over `haystack`.
fn starts_with_ignore_ascii_case(haystack: &str, prefix: &str) -> bool {
    haystack
        .get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// Trims trailing sentence punctuation off `raw`, leaving the URL token.
///
/// Markdown closers never reach here: [`url_body_end`] already stopped the
/// body before them. A balanced `)` is URL content and stays.
fn trim_tail(raw: &str) -> &str {
    raw.trim_end_matches(is_sentence_punctuation)
}

/// Byte offset just past the URL body starting at `body_start`.
///
/// The body ends at whitespace or at a markdown closer: `>`, a backtick, or
/// a `)` or `]`. Such a closer must close an enclosing construct rather than
/// a URL-internal pair.
///
/// Brackets and parentheses balanced inside the body stay, so
/// `https://a.test/x_(y)` and an IPv6 host such as `https://[::1]/x` keep
/// their inner closers.
fn url_body_end(line: &str, body_start: usize) -> usize {
    let mut parens: usize = 0;
    let mut brackets: usize = 0;
    for (offset, ch) in line[body_start..].char_indices() {
        match ch {
            ch if ch.is_whitespace() || ch == '`' || ch == '>' => return body_start + offset,
            '(' => parens += 1,
            ')' if parens == 0 => return body_start + offset,
            ')' => parens -= 1,
            '[' => brackets += 1,
            ']' if brackets == 0 => return body_start + offset,
            ']' => brackets -= 1,
            _ => {}
        }
    }
    line.len()
}

/// Trailing sentence punctuation that belongs to the surrounding prose, not
/// the URL.
fn is_sentence_punctuation(ch: char) -> bool {
    matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '`')
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// `scheme_at` accepts a listed scheme only at a token boundary.
    #[rstest]
    #[case::line_start("https://example.test", 0, Some("https://"))]
    #[case::after_space("see http://example.test", 4, Some("http://"))]
    #[case::uppercase("SEE HTTPS://example.test", 4, Some("https://"))]
    #[case::mailto("mail mailto:team@example.test", 5, Some("mailto:"))]
    #[case::nexus("see nxm://example.test/mods/1", 4, Some("nxm://"))]
    #[case::reloaded_2("see r2:mods/1", 4, Some("r2:"))]
    #[case::mid_word("xhttps://example.test", 1, None)]
    #[case::compound_scheme("git+https://example.test", 4, None)]
    #[case::relative_path("see ./docs/notes.md", 4, None)]
    #[case::unknown_scheme("see gopher://example.test", 4, None)]
    fn scheme_at_should_match_only_boundary_schemes(
        #[case] line: &str,
        #[case] index: usize,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(scheme_at(line, index), expected);
    }

    /// `trailing_range` covers the URL token alone, so trailing punctuation,
    /// markdown closers, and mid-line URLs behave differently.
    #[rstest]
    #[case::bare("See https://example.test/a/b", Some("https://example.test/a/b"))]
    #[case::sentence_end("See https://example.test/a.", Some("https://example.test/a"))]
    #[case::link_destination("[docs](https://example.test/a)", Some("https://example.test/a"))]
    #[case::autolink("See <https://example.test/a>", Some("https://example.test/a"))]
    #[case::balanced_paren(
        "See https://en.wikipedia.org/wiki/Foo_(bar)",
        Some("https://en.wikipedia.org/wiki/Foo_(bar)")
    )]
    #[case::balanced_paren_in_link(
        "[docs](https://example.test/x_(y))",
        Some("https://example.test/x_(y)")
    )]
    #[case::ipv6_host("See https://[::1]/status", Some("https://[::1]/status"))]
    #[case::code_span("See `https://example.test/a`", Some("https://example.test/a"))]
    #[case::trailing_space("See https://example.test/a ", Some("https://example.test/a"))]
    #[case::mailto("Mail mailto:team@example.test", Some("mailto:team@example.test"))]
    #[case::nexus("See nxm://example.test/mods/1", Some("nxm://example.test/mods/1"))]
    #[case::reloaded_2("See r2:mods/1", Some("r2:mods/1"))]
    #[case::mid_line("See https://example.test/a here", None)]
    #[case::autolink_then_prose("See <https://example.test/a>more", None)]
    #[case::link_then_prose("See [docs](https://example.test/a)more", None)]
    #[case::code_span_then_prose("See `https://example.test/a`more", None)]
    #[case::empty_destination("See https://", None)]
    #[case::empty_colon_scheme("See mailto:", None)]
    #[case::empty_colon_scheme_direct("See r2:", None)]
    #[case::unknown_scheme("See gopher://example.test", None)]
    fn trailing_range_should_cover_only_the_url(
        #[case] line: &str,
        #[case] expected: Option<&str>,
    ) {
        let found = trailing_range(line).map(|range| &line[range]);

        assert_eq!(found, expected);
    }
}
