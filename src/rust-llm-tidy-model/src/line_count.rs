//! Line-frequency multiset used by the line-preservation safety check.
//!
//! `count_lines` counts non-blank lines of a source string into an `AHashMap`
//! keyed by the borrowed `&str` slice, avoiding the per-line `String`
//! allocation the previous `HashMap<String, _>` incurred. Keys borrow from the
//! source string, so the returned map must not outlive it.

use ahash::AHashMap;

/// Frequency multiset of non-blank lines of `source`, keyed by borrowed slices.
///
/// Whitespace-only lines are skipped. Each key borrows from `source`.
///
/// # Arguments
///
/// - `source`: the text whose non-blank lines are counted. The returned map's
///   keys borrow from `source`, so `source` must outlive the map.
pub fn count_lines(source: &str) -> AHashMap<&str, usize> {
    // Capacity heuristic: one entry per ~24 bytes covers typical line lengths
    // without a second full pass over the source (the previous code called
    // `source.lines().count()`, which scanned the whole string just to size the
    // map). Over-estimating slightly is harmless; under-estimating triggers a
    // single regrow.
    let estimate = source.len() / 24 + 1;
    let mut map = AHashMap::with_capacity(estimate);
    for line in source.lines() {
        if line.trim().is_empty() {
            continue;
        }
        *map.entry(line).or_insert(0) += 1;
    }
    map
}
