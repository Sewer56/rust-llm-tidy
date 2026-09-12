//! Preserve input eligibility while remapping lines after transformations.
//!
//! Match exact line text, including endings, and count duplicates on both sides.
//! Admit only text whose input copies were all eligible and whose output count
//! has not increased; oversized output has no eligible lines.

use super::{ChangedLines, MAX_SOURCE_BYTES, MAX_SOURCE_LINES};
use std::collections::HashMap;

/// Current source and Git eligibility captured before any tool mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedLineSnapshot {
    /// Exact UTF-8 input, including its line endings.
    pub source: Box<str>,
    /// Eligible lines in [`Self::source`], not in transformed coordinates.
    pub changed: ChangedLines,
}

impl ChangedLineSnapshot {
    /// Map exact unchanged or moved lines into transformed coordinates.
    ///
    /// Unchanged source retains its original eligibility. Otherwise all input
    /// occurrences of a line's exact text must have been eligible.
    ///
    /// Output must have no more occurrences than input. Line endings are part of
    /// the match. No original coordinates are reused after an edit.
    ///
    /// # Arguments
    ///
    /// - `transformed` - the source text after the tool's mutation, whose
    ///   lines are matched by exact text.
    ///
    /// # Returns
    ///
    /// Eligible lines in `transformed` coordinates; empty when either text
    /// exceeds the size limits.
    ///
    /// # Remarks
    /// This deliberately permits false negatives: edited lines, mixed-eligibility
    /// duplicates, and increased duplicate counts are excluded.
    ///
    /// A text-only map cannot distinguish a moved line from an identical
    /// replacement. It never admits new text or increased copies of eligible text.
    ///
    /// Sources or output exceeding [`MAX_SOURCE_BYTES`] or [`MAX_SOURCE_LINES`]
    /// return empty eligibility. Work is bounded text scans, line lookups, and
    /// range sorting; this is not a syntax-aware provenance map.
    pub fn remap(&self, transformed: &str) -> ChangedLines {
        if self.source.len() > MAX_SOURCE_BYTES
            || transformed.len() > MAX_SOURCE_BYTES
            || self.source.lines().count() > MAX_SOURCE_LINES
            || transformed.lines().count() > MAX_SOURCE_LINES
        {
            return ChangedLines::empty();
        }
        if transformed == self.source.as_ref() {
            return self.changed.clone();
        }

        // Count both sides before admitting duplicates, so inserted copies cannot
        // inherit eligibility simply by sharing text with an input line.
        let mut candidates: HashMap<&str, (usize, usize, bool)> = HashMap::new();
        for (index, line) in self.source.split_inclusive('\n').enumerate() {
            let entry = candidates.entry(line).or_insert((0, 0, true));
            entry.0 += 1;
            entry.2 &= self.changed.overlaps(index + 1, index + 1);
        }
        for line in transformed.split_inclusive('\n') {
            if let Some(entry) = candidates.get_mut(line) {
                entry.1 += 1;
            }
        }

        ChangedLines::new(
            transformed
                .split_inclusive('\n')
                .enumerate()
                .filter(|(_, line)| {
                    candidates
                        .get(line)
                        .is_some_and(|&(input, output, changed)| changed && output <= input)
                })
                .map(|(index, _)| index + 1..=index + 1),
        )
    }
}
