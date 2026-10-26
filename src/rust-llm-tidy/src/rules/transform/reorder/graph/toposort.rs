//! Topological sort of named items by reference edges (Kahn's algorithm).
//!
//! [`toposort`] returns a reading order where callers precede callees;
//! [`TieBreak`] controls how zero-in-degree and cycle nodes are ordered.
//! `main` is always seeded first regardless of in-degree.
//!
//! The per-phase sorts in [`compute_order`] call this
//! with the tie-break their language profile selected via
//! [`PhaseStrategy::Dependency`].
//!
//! [`compute_order`]: crate::rules::transform::reorder::graph::compute_order
//! [`PhaseStrategy::Dependency`]: crate::rules::transform::reorder::graph::PhaseStrategy::Dependency

use core::cmp::Reverse;
use std::collections::BinaryHeap;

/// Tracks first references in caller output order across related groups.
pub(super) struct ReferenceOrder {
    targets: Vec<Vec<usize>>,
    ranks: Vec<usize>,
    next: usize,
}

/// Tie-breaking strategy for topological sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TieBreak {
    /// Sort zero-in-degree nodes alphabetically by name.
    Alphabetical,

    /// Keep original file order for zero-in-degree nodes.
    Stable,
}

impl ReferenceOrder {
    /// Creates unranked reference state for file or member indices.
    ///
    /// # Arguments
    ///
    /// - `len`: number of items or members.
    /// - `edges`: reference-ordered pairs; out-of-range endpoints are ignored.
    ///
    /// # Returns
    ///
    /// State that assigns each target a rank when its first caller is emitted.
    pub(super) fn new(len: usize, edges: &[(usize, usize)]) -> Self {
        let mut targets = vec![Vec::new(); len];
        for &(from, to) in edges {
            if from < len && to < len {
                targets[from].push(to);
            }
        }
        Self {
            targets,
            ranks: vec![usize::MAX; len],
            next: 0,
        }
    }

    fn record(&mut self, caller: usize) {
        // Callers arrive in output order; targets retain their reference order.
        for &target in &self.targets[caller] {
            if self.ranks[target] == usize::MAX {
                self.ranks[target] = self.next;
                self.next += 1;
            }
        }
    }
}

/// Compute a topological ordering of item indices by reference dependencies.
///
/// - `fns`: the list of item names (borrowed) in original file order within a
///   phase. The parameter name reflects original function-oriented use; it
///   works for any named item type.
/// - `edges`: a set of `(referencer_position, referenced_position)` pairs,
///   already filtered to positions within this phase.
/// - `tie_break`: controls ordering of zero-in-degree nodes and cycle nodes.
///
/// Returns a permutation vector `order` where `order[i]` is the index into
/// `fns` of the item that should appear at position `i`.
///
/// # Arguments
///
/// - `fns` - the item names in original file order (borrowed); index `i`
///   corresponds to the value being sorted.
/// - `edges` - reference dependency edges as `(referencer_position,
///   referenced_position)` pairs, already filtered to positions within this
///   phase.
/// - `tie_break` - controls ordering of zero-in-degree nodes and cycle nodes.
///
/// # Returns
///
/// The permutation: `order[i]` is the `fns` index of the item at output
/// position `i`.
///
/// # Ordering guarantees
///
/// 1. **Entry points first.** `main` sorts before all other functions
///    regardless of in-degree. Other functions with zero in-degree also
///    sort before remaining functions.
/// 2. **Callers before callees.** If `foo` calls `bar`, `foo` appears before
///    `bar` in the output.
/// 3. **Mutual recursion preserved.** Functions in a cycle stay as a contiguous
///    block. With `Alphabetical` tie-break, cycles are sorted alphabetically.
/// 4. **Unrelated functions stable.** Functions with no calls between them
///    are ordered by the tie-break strategy.
pub fn toposort(fns: &[&str], edges: &[(usize, usize)], tie_break: TieBreak) -> Vec<usize> {
    with_references(fns, edges, tie_break, None)
}

/// Sorts dependencies with optional first-reference preferences.
///
/// # Arguments
///
/// - `fns`: group-local item names in source order.
/// - `edges`: group-local reference dependency pairs.
/// - `tie_break`: fallback order for unranked, equally ranked, or cyclic nodes.
/// - `references`: local-to-global index map and shared reference state.
///
/// # Returns
///
/// A permutation of group-local positions, with `main` first and cycles last.
/// Without reference state, ready nodes emit in batches as in [`toposort`].
pub(super) fn with_references(
    fns: &[&str],
    edges: &[(usize, usize)],
    tie_break: TieBreak,
    mut references: Option<(&[usize], &mut ReferenceOrder)>,
) -> Vec<usize> {
    let n = fns.len();

    // Build adjacency list: caller -> Vec<callee>. Edges are already phase
    // positions, so no name-string lookup is needed here.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for &(caller, callee) in edges {
        if caller < n && callee < n {
            adj[caller].push(callee);
            in_degree[callee] += 1;
        }
    }

    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut visited = vec![false; n];

    // ── Phase 1: entry points first ──────────────────────────────────
    // Seed with `main` first, regardless of in-degree.
    if let Some(main_idx) = fns.iter().position(|&nm| nm == "main")
        && !visited[main_idx]
    {
        order.push(main_idx);
        visited[main_idx] = true;
        if let Some((group, references)) = &mut references {
            references.record(group[main_idx]);
        }
        for &callee in &adj[main_idx] {
            if in_degree[callee] > 0 {
                in_degree[callee] -= 1;
            }
        }
    }

    // ── Phase 2: standard Kahn iteration ─────────────────────────────
    let key = |i: usize, rank: usize| {
        Reverse((
            rank,
            if tie_break == TieBreak::Alphabetical {
                fns[i]
            } else {
                ""
            },
            i,
        ))
    };
    let mut ready = BinaryHeap::new();
    if let Some((group, references)) = &references {
        for i in 0..n {
            if !visited[i] && in_degree[i] == 0 {
                ready.push(key(i, references.ranks[group[i]]));
            }
        }
    }
    let mut zero_degree = Vec::new();
    let mut changed = true;
    while changed {
        changed = false;

        zero_degree.clear();
        if references.is_some() {
            // Pick one so a newly ready helper can beat an unrelated root.
            if let Some(Reverse((_, _, i))) = ready.pop() {
                zero_degree.push(i);
            }
        } else {
            zero_degree.extend((0..n).filter(|&i| !visited[i] && in_degree[i] == 0));
            if tie_break == TieBreak::Alphabetical {
                zero_degree.sort_by(|&a, &b| fns[a].cmp(fns[b]));
            }
        }

        for &i in &zero_degree {
            visited[i] = true;
            changed = true;
            order.push(i);
            if let Some((group, references)) = &mut references {
                references.record(group[i]);
            }

            for &callee in &adj[i] {
                if in_degree[callee] > 0 {
                    in_degree[callee] -= 1;
                    if in_degree[callee] == 0
                        && !visited[callee]
                        && let Some((group, references)) = &references
                    {
                        // All same-group callers have finished, so this rank is final.
                        ready.push(key(callee, references.ranks[group[callee]]));
                    }
                }
            }
        }
    }

    // ── Phase 3: remaining nodes (cycles) in sorted order ────────────
    {
        let mut remaining: Vec<usize> = (0..n).filter(|&i| !visited[i]).collect();
        match tie_break {
            TieBreak::Alphabetical => {
                remaining.sort_by(|&a, &b| fns[a].cmp(fns[b]));
            }
            TieBreak::Stable => {
                // already in index order (file order)
            }
        }
        if let Some((group, references)) = &mut references {
            for &i in &remaining {
                references.record(group[i]);
            }
        }
        order.extend(remaining);
    }

    order
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A → B → C linear chain: A calls B, B calls C.
    /// Expected order: A, B, C.
    #[test]
    fn test_linear_chain() {
        let fns = vec!["a", "b", "c"];
        // (a,b)=(0,1), (b,c)=(1,2)
        let edges: Vec<(usize, usize)> = vec![(0, 1), (1, 2)];

        let order = toposort(&fns, &edges, TieBreak::Stable);

        assert_eq!(order, vec![0, 1, 2]);
    }

    /// Diamond: A calls B and C; B and C both call D.
    #[test]
    fn test_diamond() {
        let fns = vec!["a", "b", "c", "d"];
        // a=0,b=1,c=2,d=3
        let edges: Vec<(usize, usize)> = vec![(0, 1), (0, 2), (1, 3), (2, 3)];

        let order = toposort(&fns, &edges, TieBreak::Stable);

        let a_pos = order.iter().position(|&i| i == 0).unwrap();
        let b_pos = order.iter().position(|&i| i == 1).unwrap();
        let c_pos = order.iter().position(|&i| i == 2).unwrap();
        let d_pos = order.iter().position(|&i| i == 3).unwrap();

        assert!(a_pos < b_pos, "a must be before b");
        assert!(a_pos < c_pos, "a must be before c");
        assert!(b_pos < d_pos, "b must be before d");
        assert!(c_pos < d_pos, "c must be before d");
    }

    /// `main` should sort first even when another function calls it.
    #[test]
    fn test_entry_point_first() {
        let fns = vec!["helper", "main"];
        // helper=0, main=1; edge (main,helper)=(1,0)
        let edges: Vec<(usize, usize)> = vec![(1, 0)];

        let order = toposort(&fns, &edges, TieBreak::Stable);

        let main_pos = order.iter().position(|&i| i == 1).unwrap();
        let helper_pos = order.iter().position(|&i| i == 0).unwrap();
        assert!(main_pos < helper_pos, "main must sort before helper");
    }

    /// A ↔ B mutual recursion.
    #[test]
    fn test_mutual_recursion_block() {
        let fns = vec!["a", "b", "c"];
        // a=0,b=1,c=2; (a,b)=(0,1), (b,a)=(1,0)
        let edges: Vec<(usize, usize)> = vec![(0, 1), (1, 0)];

        let order = toposort(&fns, &edges, TieBreak::Stable);

        let c_pos = order.iter().position(|&i| i == 2).unwrap();
        let a_pos = order.iter().position(|&i| i == 0).unwrap();
        let b_pos = order.iter().position(|&i| i == 1).unwrap();

        assert!(
            c_pos < a_pos,
            "c (zero in-degree) must appear before cycle block"
        );
        assert!(
            c_pos < b_pos,
            "c (zero in-degree) must appear before cycle block"
        );

        let diff = a_pos.abs_diff(b_pos);
        assert_eq!(diff, 1, "a and b must be adjacent (mutual recursion block)");
        assert!(
            a_pos < b_pos,
            "a must stay before b (original file order in cycle)"
        );
    }

    /// Functions with no calls between them should keep their original file order.
    #[test]
    fn test_unrelated_fns_stable() {
        let fns = vec!["a", "b", "c"];
        let edges: Vec<(usize, usize)> = vec![];

        let order = toposort(&fns, &edges, TieBreak::Stable);
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// `toposort` with `Alphabetical` tie-break sorts independent items by name.
    #[test]
    fn test_toposort_alphabetical_tie_break() {
        let fns = vec!["zebra", "alpha", "moon"];
        let edges: Vec<(usize, usize)> = vec![];

        let order = toposort(&fns, &edges, TieBreak::Alphabetical);

        let names: Vec<&str> = order.iter().map(|&i| fns[i]).collect();
        assert_eq!(names, vec!["alpha", "moon", "zebra"]);
    }

    /// `toposort` with `Stable` tie-break preserves original file order for
    /// independent items.
    #[test]
    fn test_toposort_stable_tie_break() {
        let fns = vec!["zebra", "alpha", "moon"];
        let edges: Vec<(usize, usize)> = vec![];

        let order = toposort(&fns, &edges, TieBreak::Stable);

        let names: Vec<&str> = order.iter().map(|&i| fns[i]).collect();
        assert_eq!(names, vec!["zebra", "alpha", "moon"]);
    }

    /// `toposort` with `Alphabetical` sorts cycle members alphabetically.
    #[test]
    fn test_toposort_alphabetical_cycles() {
        let fns = vec!["z", "a", "m"];
        // z=0, a=1, m=2; (z,a)=(0,1), (a,z)=(1,0)
        let edges: Vec<(usize, usize)> = vec![(0, 1), (1, 0)];

        let order = toposort(&fns, &edges, TieBreak::Alphabetical);

        let names: Vec<&str> = order.iter().map(|&i| fns[i]).collect();
        // m has zero in-degree, appears first (alphabetically it's the only one)
        assert_eq!(names[0], "m");
        // a and z form a cycle, alphabetical: a before z
        assert_eq!(names[1], "a");
        assert_eq!(names[2], "z");
    }

    /// File with no cross-function calls: identity ordering.
    #[test]
    fn test_no_calls() {
        let fns = vec!["main", "helper_a", "helper_b"];
        let edges: Vec<(usize, usize)> = vec![];

        let order = toposort(&fns, &edges, TieBreak::Stable);

        assert_eq!(order[0], 0, "main should be first");
        let remaining: Vec<usize> = order[1..].to_vec();
        assert_eq!(remaining, vec![1, 2]);
    }
}
