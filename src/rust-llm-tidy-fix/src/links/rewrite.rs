//! Two-pass core for [`super::fix_links`]: tally eligible inline links, rewrite
//! hoisted ones to reference form, and append the trailing `[text]: url`
//! definitions.

use super::scan::parse_inline_link;
use std::collections::{HashMap, HashSet};

/// Append hoisted `[text]: url` definitions at the end of `buf`, each on its
/// own line using the source's dominant line ending (`le`), so a CRLF
/// document stays CRLF after hoisting. Ensures `buf` ends with a newline so
/// the first definition starts on its own line; if the document already ends
/// with a reference definition the new definitions continue that block
/// contiguously.
pub(super) fn append_definitions(buf: &mut String, hoist: &[(&str, &str)], le: &str) {
    if !buf.ends_with('\n') {
        buf.push_str(le);
    }
    for &(text, url) in hoist {
        buf.push('[');
        buf.push_str(text);
        buf.push_str("]: ");
        buf.push_str(url);
        buf.push_str(le);
    }
}

/// Return the dominant line ending used in `source`: `"\r\n"` when CRLF is at
/// least as common as LF, otherwise `"\n"`.
///
/// # Arguments
///
/// - `source`: the text whose line endings are tallied.
///
/// Mirrors `rust_llm_tidy_model::line_endings::dominant_line_ending`.
/// Duplicated to keep this crate zero-dependency; keep in sync.
pub(super) fn dominant_line_ending(source: &str) -> &'static str {
    let crlf = source.matches("\r\n").count();
    let lf = source.matches('\n').count().saturating_sub(crlf);
    if crlf > 0 && crlf >= lf { "\r\n" } else { "\n" }
}

/// Rewrite eligible inline links in `body` to `[text]`, then re-attach `prefix`
/// and `term`. Returns `Some((new_segment, touched))` if any link was
/// rewritten, else `None` (caller emits the original segment verbatim).
/// `touched` lists each hoisted `(text, url)` pair rewritten on this line, so
/// the caller can anchor each pair at its first rewritten line.
///
/// Output is allocated lazily: only once the first hoisted link is found. If
/// no link in `body` is hoisted, returns `None` with zero allocation. `last`
/// tracks how far the verbatim prefix of `body` has been emitted; non-hoisted
/// inline links leave `last` alone so their bytes are emitted verbatim in a
/// later gap (or the trailing copy), exactly like the eager version.
pub(super) fn rewrite_links<'a>(
    prefix: &str,
    body: &'a str,
    term: &str,
    hoist: &HashSet<(&'a str, &'a str)>,
) -> Option<(String, Vec<(&'a str, &'a str)>)> {
    let mut out: Option<String> = None;
    let mut last = 0usize;
    let mut i = 0usize;
    let mut touched: Vec<(&str, &str)> = Vec::new();
    while let Some(rel) = body[i..].find('[') {
        let open = i + rel;
        match parse_inline_link(body, open) {
            Some((text, url, end)) if hoist.contains(&(text, url)) => {
                let o = out.get_or_insert_with(|| {
                    let mut s = String::with_capacity(prefix.len() + body.len() + term.len());
                    s.push_str(prefix);
                    s
                });
                o.push_str(&body[last..open]);
                o.push('[');
                o.push_str(text);
                o.push(']');
                touched.push((text, url));
                last = end;
                i = end;
            }
            // Inline link but not hoisted: skip past it. `last` is unchanged so
            // its verbatim bytes are emitted in a later gap (or trailing copy).
            Some((_text, _url, end)) => i = end,
            // Lone `[` that is not an inline link: step one byte (ASCII
            // boundary) and continue the search.
            None => i = open + 1,
        }
    }
    let mut o = out?;
    o.push_str(&body[last..]);
    o.push_str(term);
    Some((o, touched))
}

/// Scan `body` for inline links `[text](url)` and tally each `(text, url)`,
/// recording first-seen order in `order`. Reference-style, autolink, and
/// whitespace-URL forms never match the inline shape, so they are skipped.
///
/// Jumps between `[` bytes with [`str::find`] instead of walking every
/// character: the cost is O(number of brackets), not O(text). `[` is ASCII, so
/// byte offsets are valid char boundaries and behavior is identical to a
/// char-by-char scan.
pub(super) fn tally_links<'a>(
    body: &'a str,
    counts: &mut HashMap<(&'a str, &'a str), usize>,
    order: &mut Vec<(&'a str, &'a str)>,
) {
    let mut i = 0usize;
    while let Some(rel) = body[i..].find('[') {
        let open = i + rel;
        if let Some((text, url, end)) = parse_inline_link(body, open) {
            let prev = counts.get(&(text, url)).copied().unwrap_or(0);
            if prev == 0 {
                order.push((text, url));
            }
            counts.insert((text, url), prev + 1);
            i = end;
            continue;
        }
        // Not an inline link: step past this `[` (ASCII, so +1 is a char
        // boundary) and continue the search.
        i = open + 1;
    }
}
