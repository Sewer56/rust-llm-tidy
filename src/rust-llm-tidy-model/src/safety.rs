// Partially vendored from rust-reorder (MIT).
// Modified based on https://github.com/umwelt-ai/rust-reorder.
// Line-multiset safety verification.

use crate::line_count::count_lines;
use crate::line_endings::dominant_line_ending;
use anyhow::{Result, bail, ensure};

/// Verify that every non-blank line in `original` appears exactly once in `output`.
///
/// This is a multiset comparison (order-independent). It catches
/// byte-slicing bugs that drop, duplicate, or corrupt lines.
/// Whitespace-only lines are ignored because the reorder pass intentionally
/// re-normalizes blank lines between item groups.
///
/// It also guards the source's dominant line ending: a CRLF -> LF flip (or
/// vice versa) is rejected even when every line's content survives, so an
/// in-place transform cannot silently change line endings.
///
/// # Algorithm
///
/// Builds one frequency map of `original`'s non-blank lines, then makes a
/// single pass over `output` decrementing counts. Any line absent from
/// `original`, or whose count is already exhausted, is an error; any residual
/// positive count afterwards is a dropped line. This is one map and two line
/// scans instead of the prior two maps and four scans.
///
/// # Errors
///
/// Returns an error if:
/// - The dominant line ending of `original` and `output` differ (CRLF vs LF).
/// - A non-blank line appears in `output` but not in `original`.
/// - A non-blank line appears more times in `output` than in `original`.
/// - A non-blank line from `original` is absent from `output` (dropped).
pub fn verify_line_preservation(original: &str, output: &str) -> Result<()> {
    let orig_le = dominant_line_ending(original);
    let out_le = dominant_line_ending(output);
    ensure!(
        orig_le == out_le,
        "line-ending mismatch: original dominant {orig_le:?} but output dominant {out_le:?}",
    );
    let mut counts = count_lines(original);

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match counts.get_mut(line) {
            None => {
                bail!(
                    "line multiset mismatch: line {:?} appears in output but not in original",
                    line,
                );
            }
            Some(0) => {
                bail!(
                    "line multiset mismatch: line {:?} appears more times in output than in original",
                    line,
                );
            }
            Some(c) => *c -= 1,
        }
    }

    // Any remaining positive count is a line dropped from the output.
    for (line, count) in &counts {
        ensure!(
            *count == 0,
            "line multiset mismatch: line {:?} appears {} time(s) in original but 0 time(s) in output",
            line,
            count,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_lines_pass() {
        assert!(verify_line_preservation("a\nb\nc\n", "a\nb\nc\n").is_ok());
    }

    #[test]
    fn reordered_lines_pass() {
        assert!(verify_line_preservation("a\nb\nc\n", "c\nb\na\n").is_ok());
    }

    #[test]
    fn dropped_line_fails() {
        assert!(verify_line_preservation("a\nb\nc\n", "a\nc\n").is_err());
    }

    #[test]
    fn duplicated_line_fails() {
        assert!(verify_line_preservation("a\nb\n", "a\nb\nb\n").is_err());
    }

    #[test]
    fn blank_lines_ignored() {
        assert!(verify_line_preservation("a\n\nb\n", "a\nb\n").is_ok());
    }

    #[test]
    fn crlf_to_lf_flip_rejected() {
        // Same content, different dominant ending: the additive guard rejects
        // a CRLF -> LF flip even though every non-blank line survives. The old
        // `str::lines()` multiset alone could not detect this.
        assert!(verify_line_preservation("a\r\nb\r\n", "a\nb\n").is_err());
    }

    #[test]
    fn crlf_preserved_reorder_passes() {
        // Reordered CRLF: content multiset equal and dominant ending preserved.
        assert!(verify_line_preservation("a\r\nb\r\n", "b\r\na\r\n").is_ok());
    }
}
