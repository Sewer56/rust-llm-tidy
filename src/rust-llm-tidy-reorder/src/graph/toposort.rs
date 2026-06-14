//! Topological sort of named items by reference edges (Kahn's algorithm).
//!
//! [`toposort`] returns a reading order where callers precede callees;
//! [`TieBreak`] controls how zero-in-degree and cycle nodes are ordered.
//! `main` is always seeded first regardless of in-degree.

/// Tie-breaking strategy for topological sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TieBreak {
    /// Sort zero-in-degree nodes alphabetically by name.
    Alphabetical,
    /// Keep original file order for zero-in-degree nodes.
    Stable,
}

/// Compute a topological ordering of item indices by reference dependencies.
///
/// `fns` is the list of item names (borrowed) in original file order within a
/// phase (parameter name reflects original function-oriented use; works for any
/// named item type). `edges` is a set of `(referencer_position,
/// referenced_position)` pairs, already filtered to positions within this phase.
/// `tie_break` controls ordering of zero-in-degree nodes and cycle nodes.
///
/// Returns a permutation vector `order` where `order[i]` is the index into
/// `fns` of the item that should appear at position `i`.
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
        for &callee in &adj[main_idx] {
            if in_degree[callee] > 0 {
                in_degree[callee] -= 1;
            }
        }
    }

    // ── Phase 2: standard Kahn iteration ─────────────────────────────
    let mut changed = true;
    while changed {
        changed = false;

        // Collect all zero-in-degree nodes
        let mut zero_degree: Vec<usize> = (0..n)
            .filter(|&i| !visited[i] && in_degree[i] == 0)
            .collect();

        // Apply tie-break ordering
        match tie_break {
            TieBreak::Alphabetical => {
                zero_degree.sort_by(|&a, &b| fns[a].cmp(fns[b]));
            }
            TieBreak::Stable => {
                // already in index order (file order)
            }
        }

        for &i in &zero_degree {
            visited[i] = true;
            changed = true;
            order.push(i);

            for &callee in &adj[i] {
                if in_degree[callee] > 0 {
                    in_degree[callee] -= 1;
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
        order.extend(remaining);
    }

    order
}

/// Extract a bare function name from a call expression's function path.
///
/// Returns `Some("foo")` for bare `foo()` calls. Returns `None` for:
/// - Qualified paths: `mod::foo()`, `crate::foo()`
/// - Method calls: `self.foo()`, `x.foo()`
/// - Associated function calls: `Foo::bar()`
/// - Closures or other expressions
#[cfg(test)]
#[allow(dead_code)]
fn extract_bare_fn_name(expr: &syn::Expr) -> Option<String> {
    let inner = if let syn::Expr::Call(call) = expr {
        &call.func
    } else {
        expr
    };
    if let syn::Expr::Path(syn::ExprPath { path, .. }) = inner
        && path.leading_colon.is_none()
        && path.segments.len() == 1
    {
        let seg = &path.segments[0];
        if seg.arguments.is_empty() {
            return Some(seg.ident.to_string());
        }
    }
    None
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

    /// Unit test for `extract_bare_fn_name`: accepts `foo()`,
    /// rejects `mod::foo()`, `self.foo()`, `Foo::bar()`.
    #[test]
    fn test_bare_calls_only() {
        fn extract_from_source(src: &str) -> Option<String> {
            if let Ok(call) = syn::parse_str::<syn::ExprCall>(src) {
                return extract_bare_fn_name(&call.func);
            }
            if let Ok(expr) = syn::parse_str::<syn::Expr>(src) {
                return extract_bare_fn_name(&expr);
            }
            None
        }

        assert_eq!(extract_from_source("foo()"), Some("foo".to_string()));
        assert_eq!(extract_from_source("bar()"), Some("bar".to_string()));
        assert_eq!(extract_from_source("mod::foo()"), None);
        assert_eq!(extract_from_source("self::foo()"), None);
        assert_eq!(extract_from_source("crate::foo()"), None);
        assert_eq!(extract_from_source("Foo::bar()"), None);
        assert_eq!(extract_from_source("self.foo()"), None);
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

    /// `toposort` with `Stable` tie-break preserves original file order for independent items.
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
