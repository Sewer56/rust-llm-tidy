// Partially vendored from rust-reorder (MIT).
// Modified based on https://github.com/umilt-ai/rust-reorder.
// Line-multiset safety verification.

use crate::line_count::count_lines;
use anyhow::{Result, bail, ensure};

/// Verify that every non-blank line in `original` appears exactly once in `output`.
///
/// This is a multiset comparison (order-independent). It catches
/// byte-slicing bugs that drop, duplicate, or corrupt lines.
/// Whitespace-only lines are ignored because the reorder pass intentionally
/// re-normalizes blank lines between item groups.
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
/// Returns an error describing the first line whose count differs between
/// `original` and `output`.
pub fn verify_line_preservation(original: &str, output: &str) -> Result<()> {
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
}
