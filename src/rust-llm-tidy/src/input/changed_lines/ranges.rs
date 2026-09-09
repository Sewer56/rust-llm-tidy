//! Represent eligible source lines as sorted inclusive ranges.
//!
//! Normalize 1-based spans by discarding invalid ranges and merging overlaps
//! and adjacency. Use binary partitioning for overlap checks, with constructors
//! for empty eligibility or every line of a source buffer.

use core::{iter::once, ops::RangeInclusive};

/// A set of 1-based inclusive line ranges, sorted and merged across adjacency.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangedLines {
    ranges: Vec<RangeInclusive<usize>>,
}

impl ChangedLines {
    /// Normalize ranges, discarding reversed ranges and ranges starting at zero.
    pub fn new(ranges: impl IntoIterator<Item = RangeInclusive<usize>>) -> Self {
        let mut ranges: Vec<_> = ranges
            .into_iter()
            .filter(|range| *range.start() > 0 && !range.is_empty())
            .collect();
        ranges.sort_unstable_by_key(|range| (*range.start(), *range.end()));

        let mut merged: Vec<RangeInclusive<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            if let Some(previous) = merged.last_mut()
                && *range.start() <= previous.end().saturating_add(1)
            {
                *previous = *previous.start()..=(*previous.end()).max(*range.end());
            } else {
                merged.push(range);
            }
        }

        Self { ranges: merged }
    }

    /// Return an empty eligibility set.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Include every source line; an empty source has no lines.
    pub fn all(source: &str) -> Self {
        Self::new(once(1..=source.lines().count()))
    }

    /// Borrow the normalized 1-based inclusive ranges.
    pub fn ranges(&self) -> &[RangeInclusive<usize>] {
        &self.ranges
    }

    /// Return whether there are no eligible lines.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Test intersection with a 1-based inclusive span; invalid spans never overlap.
    pub fn overlaps(&self, start: usize, end: usize) -> bool {
        if start == 0 || start > end {
            return false;
        }

        let index = self.ranges.partition_point(|range| *range.end() < start);
        self.ranges
            .get(index)
            .is_some_and(|range| *range.start() <= end)
    }
}
