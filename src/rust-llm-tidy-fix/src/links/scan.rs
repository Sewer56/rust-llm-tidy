//! Line-level recognition for [`super::fix_links`].
//!
//! Helpers shared by both the tally and rewrite passes:
//! - [`step_fence`]: advance the open-fence stack for a doc-prefix-stripped
//!   line, reusing [`crate::fences::parse_fence`] for delimiter recognition.
//! - [`parse_inline_link`]: recognize a hoist-eligible inline `[text](url)`
//!   link (flat, non-blank text only).
//! - [`definition_text`]: recognize a `[text]: url` reference definition.
//! - [`doc_block_key`]: classify a raw line into the doc-comment block it
//!   belongs to, so the rewrite pass keeps per-comment definitions in lock-step
//!   with per-comment rewrites.

use crate::fences::parse_fence;
use memchr::{memchr_iter, memchr3};

/// One parsed inline link and its byte span within a line body.
pub(super) struct InlineLink<'a> {
    pub(super) text: &'a str,
    pub(super) url: &'a str,
    pub(super) open: usize,
    pub(super) end: usize,
}

/// If `body` is a complete reference-definition line (`[text]: url` plus an
/// optional title), return the link `text`. Otherwise return `None`.
///
/// Shares [`parse_definition`] with [`is_reference_definition`], so a
/// definition-shaped but malformed line never registers an existing
/// definition. The leading-`[` gate keeps the common `[`-bearing prose line
/// out of the (too large to inline) full parser.
#[inline]
pub(super) fn definition_text(body: &str) -> Option<&str> {
    // Leading-`[` gate on the trimmed line keeps the common `[`-bearing
    // prose line out of the (too large to inline) full parser.
    let s = body.trim_start();
    if !s.starts_with('[') {
        return None;
    }
    parse_definition(s)
}

/// The doc-comment block key for a line with doc `prefix`.
///
/// Lines outside any `///` / `//!` doc comment return `None`. A line belongs
/// to the doc-comment block identified by `Some(prefix)`, and a block is the
/// maximal run of consecutive lines sharing the same `Some(prefix)`. The
/// rewrite pass uses this to keep each rustdoc comment's definitions inside
/// the same block.
#[inline]
pub(super) fn doc_block_key(prefix: &str) -> Option<&str> {
    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

/// Iterate hoist-eligible inline links in one body (see
/// [`parse_inline_link`] for the label rule), resuming one byte past each
/// rejected `[` so a badge's declined outer link still yields its flat inner
/// image.
#[inline]
pub(super) fn inline_links(body: &str) -> impl Iterator<Item = InlineLink<'_>> {
    let mut next = 0usize;
    std::iter::from_fn(move || {
        while let Some(relative) = body[next..].find('[') {
            let open = next + relative;
            if let Some((text, url, end)) = parse_inline_link(body, open) {
                next = end;
                return Some(InlineLink {
                    text,
                    url,
                    open,
                    end,
                });
            }
            next = open + 1;
        }
        None
    })
}

/// True when `body` is a complete CommonMark link reference definition:
/// `[label]: destination` plus an optional quoted or parenthesized title, with
/// nothing else on the line. Shares [`parse_definition`] with
/// [`definition_text`], so both agree on what counts as a definition.
pub(super) fn is_reference_definition(body: &str) -> bool {
    parse_definition(body.trim_start()).is_some()
}

/// Iterate line segments as `(start, segment)`, retaining each terminator.
/// Uses `memchr` so every input byte participates in one vectorized newline
/// search instead of `str::split`'s character-pattern state machine.
#[inline]
pub(super) fn line_segments(input: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = 0usize;
    memchr_iter(b'\n', input.as_bytes())
        .map(|newline| newline + 1)
        .chain(std::iter::once(input.len()))
        .filter_map(move |end| {
            if end == start {
                return None;
            }
            let segment_start = start;
            start = end;
            Some((segment_start, &input[segment_start..end]))
        })
}

/// Update the open-fence stack for the (doc-prefix-stripped) line `body` and
/// report whether it is a fence delimiter line (an opener or its closer).
/// Reuses the byte-exact [`crate::fences::parse_fence`] for recognition.
///
/// Follows CommonMark block structure: while a fence is open, every other
/// marker run is code-block content, so a `~~~` line inside a backtick fence
/// (or a too-short run of the same marker) neither opens nor closes anything
/// and the original fence still closes on its own delimiter. The stack
/// therefore holds at most one entry; it stays a `Vec` because callers test it
/// with `is_empty`.
///
/// `body` is the result of [`crate::tables::strip_doc_prefix`], so the `///` /
/// `//!` marker (and its indent) is already gone; only an optional inner indent
/// may remain.
pub(super) fn step_fence(stack: &mut Vec<(char, usize)>, body: &str) -> bool {
    // Cheap candidate check: after leading whitespace, a fence must start with
    // a backtick/tilde run. Non-ASCII-leading lines defer to the full Unicode
    // `trim_start` (sound superset gate, identical to `fix_fences`'s
    // `is_fence_candidate`), so typical code/prose lines skip the pipeline.
    if !is_fence_candidate_body(body) {
        return false;
    }
    let stripped = body.trim_start();
    let Some((marker, run_len, info)) = parse_fence(stripped) else {
        return false;
    };
    match stack.last() {
        // A fence is open: only a matching closer (same marker, run at least as
        // long as the opener's, empty info string) ends it. Anything else is
        // block content, so the marker run is ignored.
        Some(&(open_marker, open_len)) => {
            let is_closer = info.is_empty() && open_marker == marker && open_len <= run_len;
            if is_closer {
                stack.pop();
            }
            is_closer
        }
        // No fence open: this run opens one.
        None => {
            stack.push((marker, run_len));
            true
        }
    }
}

/// If `body` at byte index `open` (`[`) opens an inline link `[text](url)`
/// eligible for hoisting, return `(text, url, end)` where `end` is one past
/// the closing `)`. Eligible text is non-blank (at least one byte that is
/// not a space or tab) and contains no `[` or `]` byte, nested or escaped;
/// [`super`] documents why.
///
/// Returns `None` for:
///
/// - reference-style forms
/// - autolink `<...>` URLs
/// - URLs containing whitespace/newline
/// - unbalanced brackets
/// - text failing the label rule
#[inline]
pub(super) fn parse_inline_link(body: &str, open: usize) -> Option<(&str, &str, usize)> {
    let bytes = body.as_bytes();
    // Walk to the matching `]`, allowing balanced nested `[ ]`, and track in
    // the same pass whether the text qualifies as a hoisted label: flat (no
    // bracket bytes inside the text) and non-blank (a non-space/tab byte).
    let mut depth = 1usize;
    let mut flat = true;
    let mut non_blank = false;
    let mut j = open + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'[' => {
                depth += 1;
                flat = false;
            }
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                flat = false;
            }
            b' ' | b'\t' => {}
            _ => non_blank = true,
        }
        j += 1;
    }
    if depth != 0 || j >= bytes.len() {
        return None;
    }
    if !flat || !non_blank {
        return None;
    }
    let close_bracket = j;
    let text = &body[open + 1..close_bracket];
    // Require `(` immediately after `]`.
    let paren_open = close_bracket + 1;
    if paren_open >= bytes.len() || bytes[paren_open] != b'(' {
        return None;
    }
    // Walk to `)`, rejecting whitespace/newline inside the URL.
    let mut k = paren_open + 1;
    while k < bytes.len() && bytes[k] != b')' {
        if matches!(bytes[k], b' ' | b'\t' | b'\n' | b'\r') {
            return None;
        }
        k += 1;
    }
    if k >= bytes.len() {
        return None;
    }
    let url = &body[paren_open + 1..k];
    if url.is_empty() || url.starts_with('<') {
        return None;
    }
    Some((text, url, k + 1))
}

/// Cheaply decide whether the doc-prefix-stripped `body` could begin a fence
/// under the Unicode `body.trim_start()` pipeline.
///
/// Sound superset gate (mirrors `fix_fences`'s `is_fence_candidate`): returns
/// `true` for every line the pipeline treats as a fence, plus a few extras the
/// pipeline emits verbatim. The common case - an ASCII line whose first
/// non-whitespace byte is not a marker run - short-circuits with a byte scan.
///
/// Whitespace in two tiers: ASCII whitespace (`0x09..=0x0d` plus `0x20`, the
/// ASCII members of [`char::is_whitespace`]) is skipped directly; a leading
/// non-ASCII byte (`>= 0x80`) may be Unicode whitespace before a fence, so such
/// lines defer to the full pipeline.
#[inline]
fn is_fence_candidate_body(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], 0x09..=0x0d | 0x20) {
        i += 1;
    }
    match bytes.get(i).copied() {
        // ASCII first byte: a fence - after the pipeline's Unicode trim - can
        // only start with a marker run.
        Some(b) if b <= 0x7f => b == b'`' || b == b'~',
        // Non-ASCII leading byte: may be Unicode whitespace before a fence;
        // defer to the full pipeline, which handles Unicode whitespace exactly.
        Some(_) => true,
        // Line was whitespace only (or empty): not a fence.
        None => false,
    }
}

/// Parse the leading-whitespace-trimmed `s` as one complete CommonMark link
/// reference definition, `[label]: destination` plus an optional quoted or
/// parenthesized title, with nothing else on the line. Returns the label. Any
/// malformed form (blank label, unescaped bracket in the label, invalid
/// destination, glued title, trailing junk) is paragraph text, not a
/// definition.
///
/// Recognition is deliberately conservative where full CommonMark needs inline
/// parsing: labels may not contain unescaped `[`; the destination must be a
/// non-empty balanced bare run (parens only backslash-escaped or balanced) or
/// an angle form, possibly empty, with no unescaped `<`/`>`; a title must be
/// preceded by
/// whitespace; a `\` escape is honored only before ASCII punctuation.
#[inline]
fn parse_definition(s: &str) -> Option<&str> {
    let after = s.strip_prefix('[')?;
    let close = closing_bracket(after)?;
    let text = &after[..close];
    // CommonMark: a label needs one character that is not a space or tab.
    if text.bytes().all(|b| matches!(b, b' ' | b'\t')) {
        return None;
    }
    let rest = after[close + 1..].strip_prefix(':')?;
    // CommonMark: whitespace after the colon is optional, so trim it away
    // whether present or not.
    let rest = rest.trim_start();
    let dest_len = parse_destination(rest)?;
    // Optional title, then only whitespace to end-of-line. A title must be
    // separated from the destination by whitespace (CommonMark), which the
    // angle form does not get for free.
    let after_dest = &rest[dest_len..];
    let tail = after_dest.trim_start();
    if !after_dest.is_empty() && tail.len() == after_dest.len() {
        return None;
    }
    if tail.is_empty() {
        return Some(text);
    }
    let close = match tail.as_bytes()[0] {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let end = closing_delimiter(&tail[1..], close)?;
    tail[end + 2..].trim().is_empty().then_some(text)
}

/// Index of the `]` closing a label opened before `after`: the first `]` not
/// preceded by a backslash escape (CommonMark). `\\]` does not close, and an
/// unescaped nested `[` can never belong to a label.
#[inline]
fn closing_bracket(after: &str) -> Option<usize> {
    let bytes = after.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b']' => return Some(i),
            b'[' => return None,
            _ => i += 1,
        }
    }
    None
}

/// Length of a title's content `tail`: the position of the first unescaped
/// `close` byte (quotes) or the position of its matching unescaped `)`
/// (parenthesized titles need balanced content).
#[inline]
fn closing_delimiter(tail: &str, close: u8) -> Option<usize> {
    let bytes = tail.as_bytes();
    let mut i = 0;
    if close == b')' {
        let mut depth = 1usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'(' => {
                    depth += 1;
                    i += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        None
    } else {
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b if b == close => return Some(i),
                _ => i += 1,
            }
        }
        None
    }
}

/// Length of the destination at the start of `rest`: an angle form `<...>`
/// (possibly empty, no unescaped `<`, closed by the first unescaped `>`)
/// or a non-empty bare run without whitespace or control characters whose
/// unescaped parentheses balance.
#[inline]
fn parse_destination(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.first() == Some(&b'<') {
        // `angle` indexes into `rest[1..]`; the closing `>` ends the
        // destination, an unescaped `<` or a missing closer invalidates it.
        let angle = &bytes[1..];
        let mut i = 0;
        while i < angle.len() {
            match angle[i] {
                b'\\' => i += 2,
                b'<' => return None,
                b'>' => return Some(i + 2),
                _ => i += 1,
            }
        }
        None
    } else {
        // Fast path: a destination without `(`, `)`, or `\` anywhere (the
        // overwhelming majority) is a plain run to the first ASCII
        // whitespace or control byte.
        if memchr3(b'(', b')', b'\\', bytes).is_none() {
            // `b' '`, `b'\t'`, and every ASCII control byte except 0x7f are
            // `<= b' '`, so one compare per byte finds the run's end.
            let end = bytes
                .iter()
                .position(|&b| b <= b' ' || b == 0x7f)
                .unwrap_or(bytes.len());
            // Empty is not a destination; an ASCII control byte invalidates
            // the whole run (whitespace or end-of-line merely ends it).
            return match bytes.get(end) {
                Some(b' ') | Some(b'\t') | None if end > 0 => Some(end),
                _ => None,
            };
        }
        let mut i = 0;
        let mut depth = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                // Only `\(` and `\)` are escapes; a `\` before anything else
                // is a literal byte.
                b'\\' if matches!(bytes.get(i + 1), Some(b'(' | b')')) => i += 2,
                b'(' => {
                    depth += 1;
                    i += 1;
                }
                b')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        // Balanced close: the destination can end here; a
                        // non-whitespace continuation keeps the run going.
                        match bytes.get(i + 1) {
                            Some(b' ') | Some(b'\t') | None => return Some(i + 1),
                            _ => {}
                        }
                    }
                    i += 1;
                }
                b' ' | b'\t' => return (i > 0).then_some(i),
                b if b < 0x20 || b == 0x7f => return None,
                _ => i += 1,
            }
        }
        (i > 0 && depth == 0).then_some(i)
    }
}
