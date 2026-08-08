//! Line-level recognition for [`super::fix_links`].
//!
//! Helpers shared by both the tally and rewrite passes:
//! - [`step_fence`]: advance the open-fence stack for a doc-prefix-stripped
//!   line, in lock-step with [`crate::fences::fix_fences`].
//! - [`parse_inline_link`]: recognize an inline `[text](url)` link.
//! - [`definition_text`]: recognize a `[text]: url` reference definition.

use crate::fences::parse_fence;

/// If `body` is a reference-definition line (`[text]: url`), return the link
/// `text`. Otherwise return `None`.
#[inline]
pub(super) fn definition_text(body: &str) -> Option<&str> {
    let s = body.trim_start();
    let after = s.strip_prefix('[')?;
    let close = after.find(']')?;
    let text = &after[..close];
    let rest = after[close + 1..].strip_prefix(':')?;
    // CommonMark requires whitespace (or end-of-line) after the colon.
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
        Some(text)
    } else {
        None
    }
}

/// If `body` at byte index `open` (`[`) opens an inline link `[text](url)`,
/// return `(text, url, end)` where `end` is one past the closing `)`.
/// Returns `None` for reference-style forms, autolink `<...>` URLs, URLs
/// containing whitespace/newline, or unbalanced brackets.
#[inline]
pub(super) fn parse_inline_link(body: &str, open: usize) -> Option<(&str, &str, usize)> {
    let bytes = body.as_bytes();
    // Walk to the matching `]`, allowing balanced nested `[ ]`.
    let mut depth = 1usize;
    let mut j = open + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if depth != 0 || j >= bytes.len() {
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

/// Update the open-fence stack for the (doc-prefix-stripped) line `body` and
/// report whether it is a fence delimiter line. Reuses the byte-exact
/// [`crate::fences::parse_fence`], so fence skipping stays in lock-step with
/// `fix_fences`.
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
    let is_closer = info.is_empty()
        && stack
            .last()
            .map(|(m, r)| *m == marker && *r <= run_len)
            .unwrap_or(false);
    if is_closer {
        stack.pop();
    } else {
        stack.push((marker, run_len));
    }
    true
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
