//! Normalize borrowed source lines and retain exact equality and query boundaries.

use crate::input::changed_lines::ChangedLines;
use ahash::AHashMap;

/// Line identities plus linear-size rolling fingerprints and meaningful positions.
pub(super) struct Lines<'a> {
    pub(super) ids: Vec<usize>,
    pub(super) meaningful: Vec<usize>,
    /// Exclusive eligible-run end for each line; zero means ineligible.
    pub(super) query_end: Vec<usize>,
    text: Vec<&'a str>,
    hashes: Vec<u64>,
    powers: Vec<u64>,
}

impl<'a> Lines<'a> {
    /// Intern normalized lines once; internal whitespace and comments remain literal.
    pub(super) fn new(source: &'a str, eligible: &ChangedLines, exact: bool) -> Self {
        let text: Vec<_> = source
            .split_inclusive('\n')
            .map(|line| {
                if exact {
                    line
                } else {
                    let line = line.strip_suffix('\n').unwrap_or(line);
                    line.strip_suffix('\r')
                        .unwrap_or(line)
                        .trim_matches([' ', '\t'])
                }
            })
            .collect();
        let mut interned = AHashMap::new();
        let mut ids = Vec::with_capacity(text.len());
        let mut meaningful = Vec::new();
        let mut hashes = Vec::with_capacity(text.len() + 1);
        let mut powers = Vec::with_capacity(text.len() + 1);
        hashes.push(0u64);
        powers.push(1u64);

        // Fingerprints only choose buckets. Equality always checks the actual lines.
        const BASE: u64 = 0x9e37_79b9_7f4a_7c15;
        for (index, &line) in text.iter().enumerate() {
            let next_id = interned.len() + 1;
            let id = *interned.entry(line).or_insert(next_id);
            ids.push(id);
            hashes.push(hashes[index].wrapping_mul(BASE).wrapping_add(id as u64));
            powers.push(powers[index].wrapping_mul(BASE));
            if line.chars().any(char::is_alphanumeric) {
                meaningful.push(index);
            }
        }

        let mut query_end = vec![0; text.len()];
        for range in eligible.ranges() {
            let end = (*range.end()).min(text.len());
            let start = (range.start() - 1).min(end);
            query_end[start..end].fill(end);
        }
        Self {
            ids,
            meaningful,
            query_end,
            text,
            hashes,
            powers,
        }
    }

    /// Fingerprint a nonempty half-open physical-line span in constant time.
    pub(super) fn fingerprint(&self, start: usize, end: usize) -> u64 {
        self.hashes[end].wrapping_sub(self.hashes[start].wrapping_mul(self.powers[end - start]))
    }

    /// Verify normalized content, never relying on a fingerprint collision result.
    pub(super) fn equal(&self, first: usize, second: usize, length: usize) -> bool {
        self.text[first..first + length] == self.text[second..second + length]
    }

    /// Locate the next meaningful endpoint without dropping intervening punctuation.
    pub(super) fn next_meaningful(&self, start: usize) -> Option<usize> {
        self.meaningful
            .get(self.meaningful.partition_point(|&line| line < start))
            .copied()
    }
}
