// Partially vendored from rust-reorder (MIT).
// Modified based on https://github.com/umwelt-ai/rust-reorder.
// Line-multiset safety verification.

use anyhow::{Result, ensure};
use std::collections::HashMap;

/// Verify that every non-blank line in `original` appears exactly once in `output`.
///
/// This is a multiset comparison (order-independent). It catches
/// byte-slicing bugs that drop, duplicate, or corrupt lines.
/// Whitespace-only lines are ignored because the reorder pass intentionally
/// re-normalizes blank lines between item groups.
///
/// # Errors
///
/// Returns an error describing the first line whose count differs between
/// `original` and `output`.
pub fn verify_line_preservation(original: &str, output: &str) -> Result<()> {
    let original_lines = count_lines(original);
    let output_lines = count_lines(output);

    // Check that every line in original appears with the same count in output.
    for (line, count) in &original_lines {
        let out_count = output_lines.get(line).copied().unwrap_or(0);
        ensure!(
            *count == out_count,
            "line multiset mismatch: line {:?} appears {} time(s) in original but {} time(s) in output",
            line,
            count,
            out_count,
        );
    }

    // Check that output doesn't have extra lines not in original.
    for (line, count) in &output_lines {
        let orig_count = original_lines.get(line).copied().unwrap_or(0);
        ensure!(
            *count == orig_count,
            "line multiset mismatch: line {:?} appears {} time(s) in output but {} time(s) in original",
            line,
            count,
            orig_count,
        );
    }

    Ok(())
}

fn count_lines(s: &str) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for line in s.lines() {
        if line.trim().is_empty() {
            continue;
        }
        *map.entry(line.to_string()).or_insert(0) += 1;
    }
    map
}
