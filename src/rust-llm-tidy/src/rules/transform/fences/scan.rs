//! Line-level fence recognition for [`super::fix_fences`].
//!
//! Two helpers:
//! - [`is_fence_candidate`]: cheap superset gate that short-circuits the
//!   common non-fence line before the full Unicode-aware pipeline runs.
//! - [`parse_fence`]: exact fence parser for a trimmed line body.

/// Cheaply decide whether `segment` could begin a fence under the full
/// [`super::strip_comment_prefix`] + Unicode `body.trim_start()` pipeline for
/// one prefix family.
///
/// This is a sound superset gate: it returns `true` for every line the pipeline
/// would treat as a fence, plus a few extras the pipeline would emit verbatim
/// (still correct).
///
/// The common case - an ASCII line whose first non-whitespace byte is not a
/// marker run or one of the family's comment markers - short-circuits with a
/// raw byte scan, so typical code and prose cost almost nothing.
///
/// Whitespace handled in two tiers to stay both exact and fast:
/// - ASCII whitespace (`0x09..=0x0d` plus space `0x20` - the ASCII members of
///   [`char::is_whitespace`]) is skipped directly; this covers the realistic
///   indentation (spaces, tabs) and the ASCII oddities like form feed.
/// - A leading non-ASCII byte (`>= 0x80`) may be Unicode whitespace (e.g.
///   NBSP, ideographic space) preceding a fence, so such lines defer to the
///   full Unicode-aware pipeline rather than risk being skipped.
#[inline]
pub(super) fn is_fence_candidate(segment: &str, prefixes: &[&str]) -> bool {
    let bytes = segment.as_bytes();
    // Skip ASCII whitespace (the ASCII subset of `char::is_whitespace`); a tight
    // byte scan. `i` lands on a `char` boundary, so slicing below is safe.
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], 0x09..=0x0d | 0x20) {
        i += 1;
    }
    match bytes.get(i).copied() {
        // ASCII first byte: a fence - after the pipeline's Unicode trim - can
        // only start with a marker run or a comment prefix from the family,
        // all ASCII.
        Some(b) if b <= 0x7f => {
            b == b'`' || b == b'~' || prefixes.iter().any(|&p| segment[i..].starts_with(p))
        }
        // Non-ASCII leading byte: may be Unicode whitespace before a fence;
        // defer to the full pipeline, which handles Unicode whitespace exactly.
        Some(_) => true,
        // Line was whitespace only (or empty): not a fence.
        None => false,
    }
}

/// Parse a fence from `stripped` if its leading run is 3+ backticks or tildes.
///
/// Returns `(marker, run_len, info)` where `info` is the text after the run
/// (may be empty). Returns `None` for non-fence lines.
#[inline]
pub(crate) fn parse_fence(stripped: &str) -> Option<(char, usize, &str)> {
    let bytes = stripped.as_bytes();
    // `stripped` is the trimmed line body; it may be empty for a blank line.
    let &marker = bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    // Backticks and tildes are ASCII, so the byte run length equals the char
    // run length and the byte offset is a valid `char` boundary.
    let run_len = bytes
        .iter()
        .position(|&c| c != marker)
        .unwrap_or(bytes.len());
    if run_len < 3 {
        return None;
    }
    let info = &stripped[run_len..];
    Some((marker as char, run_len, info))
}
