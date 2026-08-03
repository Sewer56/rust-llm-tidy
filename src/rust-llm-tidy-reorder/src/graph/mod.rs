//! Reference-graph ordering of Rust source files into a reading order.
//!
//! [`compute_order`] is the entry point: it collects intra-file reference
//! edges, then topologically sorts each item-kind phase to produce a
//! `Vec<usize>` permutation of the parsed items that puts callers before
//! callees (and macro/impl definitions before their uses).

use ahash::{AHashMap, AHashSet};
pub use collect::ReferenceCollector;
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, VisibilityTier};
pub use toposort::{TieBreak, toposort};

mod collect;
mod toposort;

/// Extract reference edges from parsed source and compute a topological ordering.
///
/// Phases (in output order):
/// 1. extern crate + Other (stable)
/// 2. use                  (stable, original order; rustfmt controls sorting)
/// 3. mod (non-test)       (stable, original order; rustfmt controls sorting)
/// 4. macro                (macro_rules! + macro 2.0; dependency → alphabetical;
///    a macro referencing another local macro follows it;
///    local top-level invocations follow their definition)
/// 5. const + static       (dependency → alphabetical)
/// 6. struct, enum, union, type (dependency → alphabetical)
/// 7. trait                (dependency → alphabetical)
/// 8. impl                 (inherent before trait; after matching type)
/// 9. fn                   (pub → pub(crate)/pub(super) → private; dependency within; main first)
/// 10. #[cfg(test)] mod    (stable, last)
///
/// Returns a `Vec<usize>` suitable for constructing a `Permutation` in the
/// reorder stage.  Each element is an index into `parsed.items`.
///
/// # Arguments
///
/// - `parsed` - the parsed Rust source whose top-level items are ordered.
///
/// # Errors
///
/// Returns an [`anyhow::Error`] on internal graph-ordering failure. Because
/// ordering operates over an already-parsed [`ParseResult`] and never
/// re-parses, this never fires for a well-formed `parsed`.
pub fn compute_order(parsed: &ParseResult) -> anyhow::Result<Vec<usize>> {
    let n = parsed.items.len();

    // ── 1. Build name -> item-index map (one entry per distinct name) for
    //        reference collection. Borrowed `&str` keys avoid cloning names.
    //        `or_insert` keeps the *first* index per name: names can collide
    //        across namespaces (e.g. a `macro_rules! foo` def and a later
    //        `foo!()` invocation share the name "foo"); keeping the def's index
    //        (which precedes its uses) matches the prior name-keyed lookup. ──
    let mut name_to_idx: AHashMap<&str, usize> = AHashMap::with_capacity(parsed.items.len());
    for (i, item) in parsed.items.iter().enumerate() {
        if let Some(name) = item.name() {
            name_to_idx.entry(name).or_insert(i);
        }
    }

    let macro_names: AHashSet<&str> = parsed
        .items
        .iter()
        .filter(|item| item.kind() == &ItemKind::Macro)
        .filter_map(|item| item.name())
        .collect();

    // ── 2. Collect reference edges, reusing the syntax tree stored in the
    //        parse result instead of re-parsing `parsed.source` ──
    let tree = parsed.syntax_tree();
    let mut collector = ReferenceCollector::new(name_to_idx, macro_names.clone());
    collector.collect(tree, parsed.source.as_bytes());
    let edges = collector.into_edges();

    // ── 3. Group items by kind. Phases hold item *indices* only; names are
    //        borrowed from `parsed.items` on demand, avoiding a per-item name
    //        clone into these vectors. ──
    let mut phase1: Vec<usize> = Vec::new(); // extern + Other
    let mut phase2: Vec<usize> = Vec::new(); // use
    let mut phase3: Vec<usize> = Vec::new(); // mod (non-test)
    let mut phase3_macro: Vec<usize> = Vec::new(); // macro defs
    let mut macro_invocations: Vec<usize> = Vec::new(); // local macro invocations
    let mut phase4: Vec<usize> = Vec::new(); // const + static
    let mut phase5: Vec<usize> = Vec::new(); // struct, enum, union, type
    let mut phase6: Vec<usize> = Vec::new(); // trait
    let mut phase7_inherent: Vec<usize> = Vec::new(); // inherent impls
    let mut phase7_trait: Vec<usize> = Vec::new(); // trait impls
    let mut phase8: Vec<usize> = Vec::new(); // fn
    let mut phase9: Vec<usize> = Vec::new(); // #[cfg(test)] mod

    for (idx, item) in parsed.items.iter().enumerate() {
        match item.kind() {
            ItemKind::Extern => phase1.push(idx),
            ItemKind::Other => phase1.push(idx),
            ItemKind::Use => phase2.push(idx),
            ItemKind::Mod => {
                if item.is_test_module() {
                    phase9.push(idx);
                } else {
                    phase3.push(idx);
                }
            }
            ItemKind::Const | ItemKind::Static => phase4.push(idx),
            ItemKind::Struct | ItemKind::Enum | ItemKind::Union | ItemKind::Type => {
                phase5.push(idx);
            }
            ItemKind::Trait => phase6.push(idx),
            ItemKind::Macro => phase3_macro.push(idx),
            ItemKind::MacroInvocation => {
                let name = item.name().unwrap_or("");
                // Only invocations of a locally-defined macro_rules! need to
                // follow their definition. External macros (println!,
                // tokio::main, ...) stay in the stable phase-1 bucket.
                if !name.is_empty() && macro_names.contains(name) {
                    macro_invocations.push(idx);
                } else {
                    phase1.push(idx);
                }
            }
            ItemKind::Impl => {
                if item.is_trait_impl() {
                    phase7_trait.push(idx);
                } else {
                    phase7_inherent.push(idx);
                }
            }
            ItemKind::Fn => phase8.push(idx),
        }
    }

    let mut final_order: Vec<usize> = Vec::with_capacity(n);

    // ── Phase 1: extern crate + Other (stable original order) ──
    final_order.extend(&phase1);

    // ── Phase 2: use (stable original order; rustfmt controls sorting) ──
    final_order.extend(&phase2);

    // ── Phase 3: mod (non-test, stable original order; rustfmt controls sorting) ──
    final_order.extend(&phase3);

    // ── Phase 4: macro defs (dependency → alphabetical), each followed by
    //    its local macro invocations in source order ──
    {
        let macro_start = final_order.len();
        sort_phase_by_dep(parsed, &phase3_macro, &edges, &mut final_order);

        // sort_phase_by_dep appended the defs as the tail of final_order.
        // Split them off and re-emit each def followed by its invocations,
        // so a `macro_rules!` definition always precedes its use sites
        // (Rust `macro_rules!` use textual scoping).
        let def_order: Vec<usize> = final_order.split_off(macro_start);

        // Group invocations by macro name, preserving source order. The loop
        // above pushed them in ascending item-index order, so the Vec is
        // already in source order.
        let mut invocations_by_def: AHashMap<&str, Vec<usize>> = AHashMap::new();
        for &inv_idx in &macro_invocations {
            let inv_name = parsed.items[inv_idx].name().unwrap_or("");
            invocations_by_def
                .entry(inv_name)
                .or_default()
                .push(inv_idx);
        }

        for &def_idx in &def_order {
            final_order.push(def_idx);
            let def_name = parsed.items[def_idx].name().unwrap_or("");
            if let Some(invocs) = invocations_by_def.get(def_name) {
                final_order.extend(invocs.iter().copied());
            }
        }

        // Orphan invocations: a local name with no matching def is impossible
        // here (we filtered on macro_names ⊆ def names), but if a def had no
        // extractable name the invocation was routed to phase 1 instead. So
        // nothing remains.
    }

    // ── Helpers for dependency-sorted phases ──

    /// Sort a phase's items by reference dependency with alphabetical tie-break.
    /// Names are borrowed from `parsed.items[phase_items[*]]`, so no per-item
    /// name clone is needed.
    fn sort_phase_by_dep(
        parsed: &ParseResult,
        phase_items: &[usize],
        edges: &[(usize, usize)],
        final_order: &mut Vec<usize>,
    ) {
        // Borrowed names for toposort's alphabetical tie-break / `main` lookup.
        let names: Vec<&str> = phase_items
            .iter()
            .map(|&idx| parsed.items[idx].name().unwrap_or(""))
            .collect();
        // item index -> position within this phase
        let mut pos_by_idx: AHashMap<usize, usize> = AHashMap::with_capacity(phase_items.len());
        for (pos, &idx) in phase_items.iter().enumerate() {
            pos_by_idx.insert(idx, pos);
        }

        // Keep only edges whose both endpoints are in this phase, translated to
        // phase positions for the toposort.
        let mut phase_edges: Vec<(usize, usize)> = Vec::new();
        for &(a, b) in edges {
            if let (Some(&pa), Some(&pb)) = (pos_by_idx.get(&a), pos_by_idx.get(&b)) {
                phase_edges.push((pa, pb));
            }
        }

        let order = toposort(&names, &phase_edges, TieBreak::Alphabetical);
        for pos in &order {
            final_order.push(phase_items[*pos]);
        }
    }

    // ── Phase 5: const + static (dependency → alphabetical) ──
    sort_phase_by_dep(parsed, &phase4, &edges, &mut final_order);

    // ── Phase 6: struct, enum, union, type (dependency → alphabetical) ──
    sort_phase_by_dep(parsed, &phase5, &edges, &mut final_order);

    // ── Phase 7: trait (dependency → alphabetical) ──
    sort_phase_by_dep(parsed, &phase6, &edges, &mut final_order);

    // ── Phase 8: impl (inherent first, then trait impls; after matching type) ──
    {
        let mut placed_inherent: AHashSet<usize> = AHashSet::new();
        let mut placed_trait: AHashSet<usize> = AHashSet::new();

        // Place inherent impls after their matching type
        for i in 0..final_order.len() {
            let type_idx = final_order[i];
            if let Some(type_name) = parsed.items[type_idx].name() {
                for &impl_idx in &phase7_inherent {
                    if placed_inherent.contains(&impl_idx) {
                        continue;
                    }
                    if let Some(impl_target) = parsed.items[impl_idx].impl_target_name()
                        && impl_target == type_name
                    {
                        final_order.push(impl_idx);
                        placed_inherent.insert(impl_idx);
                    }
                }
            }
        }

        // Place trait impls after their matching type (and after inherent impls for same type)
        for i in 0..final_order.len() {
            let type_idx = final_order[i];
            if let Some(type_name) = parsed.items[type_idx].name() {
                for &impl_idx in &phase7_trait {
                    if placed_trait.contains(&impl_idx) {
                        continue;
                    }
                    if let Some(impl_target) = parsed.items[impl_idx].impl_target_name()
                        && impl_target == type_name
                    {
                        final_order.push(impl_idx);
                        placed_trait.insert(impl_idx);
                    }
                }
            }
        }

        // Orphan impls: inherent first, then trait, stable order within
        for &impl_idx in &phase7_inherent {
            if !placed_inherent.contains(&impl_idx) {
                final_order.push(impl_idx);
            }
        }
        for &impl_idx in &phase7_trait {
            if !placed_trait.contains(&impl_idx) {
                final_order.push(impl_idx);
            }
        }
    }

    // ── Phase 9: fn (visibility groups; then dependency within each group; main first) ──
    {
        // Group fns by visibility (indices only; names borrowed later).
        let mut pub_fns: Vec<usize> = Vec::new();
        let mut restricted_fns: Vec<usize> = Vec::new();
        let mut private_fns: Vec<usize> = Vec::new();

        for &idx in &phase8 {
            match parsed.items[idx].visibility() {
                Some(VisibilityTier::Pub) => pub_fns.push(idx),
                Some(VisibilityTier::PubRestricted) => restricted_fns.push(idx),
                _ => private_fns.push(idx),
            }
        }

        // Sort each visibility group
        for group in [pub_fns, restricted_fns, private_fns] {
            sort_fn_group(parsed, &group, &edges, &mut final_order);
        }
    }

    // ── Phase 10: #[cfg(test)] mod (stable original order) ──
    final_order.extend(&phase9);

    Ok(final_order)
}

/// Helper: sort a group of fns by dependency with alphabetical tie-break.
/// `main`-first is handled by `toposort` Phase 1; this helper only groups by
/// visibility. Names are borrowed from `parsed.items`.
fn sort_fn_group(
    parsed: &ParseResult,
    group: &[usize],
    edges: &[(usize, usize)],
    final_order: &mut Vec<usize>,
) {
    let names: Vec<&str> = group
        .iter()
        .map(|&idx| parsed.items[idx].name().unwrap_or(""))
        .collect();
    let mut pos_by_idx: AHashMap<usize, usize> = AHashMap::with_capacity(group.len());
    for (pos, &idx) in group.iter().enumerate() {
        pos_by_idx.insert(idx, pos);
    }

    let mut group_edges: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in edges {
        if let (Some(&pa), Some(&pb)) = (pos_by_idx.get(&a), pos_by_idx.get(&b)) {
            group_edges.push((pa, pb));
        }
    }

    let order = toposort(&names, &group_edges, TieBreak::Alphabetical);
    for pos in &order {
        final_order.push(group[*pos]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent `macro_rules!` definitions are sorted alphabetically.
    #[test]
    fn test_macros_sorted_alphabetically() {
        let source = r#"
            macro_rules! b { () => {}; }
            macro_rules! a { () => {}; }
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // Source order: 0 = macro b, 1 = macro a. Alphabetical: a, b.
        assert_eq!(order, vec![1, 0]);
    }

    /// `macro_rules!` definitions sort before functions that invoke them.
    #[test]
    fn test_compute_order_macro_before_function() {
        let source = r#"
            fn b() { a!(); }
            macro_rules! a { () => {}; }
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // Source order: 0 = fn b, 1 = macro a. Macro should be first.
        assert_eq!(order, vec![1, 0]);
    }

    /// A top-level macro invocation follows its `macro_rules!` definition,
    /// even when `use`/`static` items sit between them in the source.
    #[test]
    fn test_top_level_invocation_after_def() {
        // Source order: 0 = use, 1 = static, 2 = macro_rules! a, 3 = a!().
        let source = r#"
            use std::fs;
            static COUNT: i32 = 0;
            macro_rules! a { () => {}; }
            a!();
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // Expected phases: use(0), macro def(2), invocation(3), static(1).
        assert_eq!(order, vec![0, 2, 3, 1]);
    }

    /// A top-level invocation of an unknown macro (no local `macro_rules!`)
    /// stays stable like other uncategorized items, not in the macro phase.
    #[test]
    fn test_external_invocation_stable() {
        // Source order: 0 = println!(), 1 = macro_rules! a.
        let source = r#"
            println!("x");
            macro_rules! a { () => {}; }
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // println! is external (no local def) -> phase 1, stays first.
        // Then macro def a -> phase 4.
        assert_eq!(order, vec![0, 1]);
    }

    /// Multiple invocations of the same macro preserve their source order and
    /// all follow the definition.
    #[test]
    fn test_multiple_invocations_preserve_source_order() {
        // Source order: 0 = a!(), 1 = b!(), 2 = macro_rules! m.
        let source = r#"
            m!(a);
            m!(b);
            macro_rules! m { ($x:ident) => {}; }
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // def(2) first, then invocations in source order: a(0), b(1).
        assert_eq!(order, vec![2, 0, 1]);
    }

    /// A `macro_rules!` body invoking another local macro records a reversed
    /// dependency edge, so the referenced macro is defined first.
    #[test]
    fn test_macro_def_calls_another_macro_def() {
        // Source order: 0 = alpha (calls bravo), 1 = bravo, 2 = alpha!().
        let source = r#"
            macro_rules! alpha {
                () => { bravo!(); };
            }
            macro_rules! bravo {
                () => {};
            }
            alpha!();
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // bravo(1) before alpha(0) (alpha depends on bravo), then invocation(2).
        assert_eq!(order, vec![1, 0, 2]);
    }

    /// A chain of three macro defs (a → b → c) sorts by dependency, not
    /// alphabetical order.
    #[test]
    fn test_macro_chain_dependency() {
        // Source order: 0 = macro a (calls b), 1 = macro b (calls c), 2 = macro c.
        let source = r#"
            macro_rules! a {
                () => { b!(); };
            }
            macro_rules! b {
                () => { c!(); };
            }
            macro_rules! c {
                () => {};
            }
        "#;

        let parsed = rust_llm_tidy_model::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // c(2), b(1), a(0): callees before callers.
        assert_eq!(order, vec![2, 1, 0]);
    }
}
