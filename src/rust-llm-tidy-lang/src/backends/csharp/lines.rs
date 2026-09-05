//! Line-index helpers for the C# parse: byte offsets of line starts,
//! line lookup, and newline-aware span ends.
//!
//! Mirrors the model crate's line helpers (`next_line_start`, `line_of`)
//! with the same semantics so span tiling behaves identically across
//! languages.

/// The first line start strictly after `pos`, clamped to `len`: the byte
/// after the line's terminating newline.
///
/// Binary search, matching the model crate's `next_line_start` semantics.
pub(super) fn end_past_newline(pos: usize, line_starts: &[usize], len: usize) -> usize {
    let next = line_starts.partition_point(|&start| start <= pos);
    line_starts.get(next).copied().unwrap_or(len)
}

/// 1-based line number of `pos`.
pub(super) fn line_of(line_starts: &[usize], pos: usize) -> usize {
    line_starts.partition_point(|&start| start <= pos)
}

/// Byte offset of the start of each line, in order.
///
/// One pass over the bytes with a heuristic capacity, matching the model
/// crate's line-index helper.
pub(super) fn line_start_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut starts = Vec::with_capacity(bytes.len() / 21 + 1);
    starts.push(0);
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

/// `pos` advanced past one line ending when one starts there.
pub(super) fn skip_one_line_ending(pos: usize, source: &str) -> usize {
    let rest = &source.as_bytes()[pos..];
    if rest.starts_with(b"\r\n") {
        pos + 2
    } else if rest.starts_with(b"\n") {
        pos + 1
    } else {
        pos
    }
}
