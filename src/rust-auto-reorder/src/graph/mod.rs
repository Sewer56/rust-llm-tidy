//! Reference-graph ordering of Rust source files into a reading order.
//!
//! [`compute_order`] is the entry point: it collects intra-file reference
//! edges, then topologically sorts each item-kind phase to produce a
//! `Vec<usize>` permutation of the parsed items that puts callers before
//! callees (and macro/impl definitions before their uses).

use crate::parse::{ItemKind, ParseResult, VisibilityTier};
pub use collect::ReferenceCollector;
use std::collections::{HashMap, HashSet};
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
/// # Errors
///
/// Returns a parse error when `parsed.source` is not valid Rust syntax.
pub fn compute_order(parsed: &ParseResult) -> anyhow::Result<Vec<usize>> {
    let n = parsed.items.len();

    // ── 1. Collect top-level names (all named items) for dependency collection ──
    let top_level_names: HashSet<String> = parsed
        .items
        .iter()
        .filter_map(|item| item.name().map(|n| n.to_string()))
        .collect();

    let macro_names: HashSet<String> = parsed
        .items
        .iter()
        .filter(|item| item.kind() == &ItemKind::Macro)
        .filter_map(|item| item.name().map(|n| n.to_string()))
        .collect();

    // ── 2. Build syn::File and collect reference edges ──
    let file: syn::File = syn::parse_str(&parsed.source)?;
    let mut collector = ReferenceCollector::new(top_level_names.clone(), macro_names.clone());
    collector.collect(&file);
    let edges = collector.into_edges();

    // ── 3. Helper: group items by kind ──
    let mut phase1: Vec<usize> = Vec::new(); // extern + Other
    let mut phase2: Vec<usize> = Vec::new(); // use
    let mut phase3: Vec<usize> = Vec::new(); // mod (non-test)
    let mut phase3_macro: Vec<(usize, String)> = Vec::new(); // macro defs
    let mut macro_invocations: Vec<(usize, String)> = Vec::new(); // local macro invocations
    let mut phase4: Vec<(usize, String)> = Vec::new(); // const + static
    let mut phase5: Vec<(usize, String)> = Vec::new(); // struct, enum, union, type
    let mut phase6: Vec<(usize, String)> = Vec::new(); // trait
    let mut phase7_inherent: Vec<usize> = Vec::new(); // inherent impls
    let mut phase7_trait: Vec<usize> = Vec::new(); // trait impls
    let mut phase8: Vec<(usize, String)> = Vec::new(); // fn
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
            ItemKind::Const | ItemKind::Static => {
                phase4.push((idx, item.name().unwrap_or("").to_string()));
            }
            ItemKind::Struct | ItemKind::Enum | ItemKind::Union | ItemKind::Type => {
                phase5.push((idx, item.name().unwrap_or("").to_string()));
            }
            ItemKind::Trait => {
                phase6.push((idx, item.name().unwrap_or("").to_string()));
            }
            ItemKind::Macro => {
                phase3_macro.push((idx, item.name().unwrap_or("").to_string()));
            }
            ItemKind::MacroInvocation => {
                let name = item.name().unwrap_or("").to_string();
                // Only invocations of a locally-defined macro_rules! need to
                // follow their definition. External macros (println!,
                // tokio::main, ...) stay in the stable phase-1 bucket.
                if !name.is_empty() && macro_names.contains(&name) {
                    macro_invocations.push((idx, name));
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
            ItemKind::Fn => {
                phase8.push((idx, item.name().unwrap_or("").to_string()));
            }
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
        sort_phase_by_dep(phase3_macro, &edges, &mut final_order);

        // sort_phase_by_dep appended the defs as the tail of final_order.
        // Split them off and re-emit each def followed by its invocations,
        // so a `macro_rules!` definition always precedes its use sites
        // (Rust `macro_rules!` use textual scoping).
        let def_order: Vec<usize> = final_order.split_off(macro_start);

        // Group invocations by macro name, preserving source order. The loop
        // above pushed them in ascending item-index order, so the Vec is
        // already in source order.
        let mut invocations_by_def: HashMap<&str, Vec<usize>> = HashMap::new();
        for (inv_idx, inv_name) in &macro_invocations {
            invocations_by_def
                .entry(inv_name.as_str())
                .or_default()
                .push(*inv_idx);
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

    fn sort_phase_by_dep(
        phase_items: Vec<(usize, String)>,
        edges: &[(String, String)],
        final_order: &mut Vec<usize>,
    ) {
        let names: Vec<String> = phase_items.iter().map(|(_, n)| n.clone()).collect();
        let name_to_pos: HashMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();

        // Filter edges to only those within this phase
        let phase_edges: Vec<(String, String)> = edges
            .iter()
            .filter(|(a, b)| {
                name_to_pos.contains_key(a.as_str()) && name_to_pos.contains_key(b.as_str())
            })
            .cloned()
            .collect();

        let order = toposort(&names, &phase_edges, TieBreak::Alphabetical);
        for pos in &order {
            let idx = phase_items[*pos].0;
            final_order.push(idx);
        }
    }

    // ── Phase 5: const + static (dependency → alphabetical) ──
    sort_phase_by_dep(phase4, &edges, &mut final_order);

    // ── Phase 6: struct, enum, union, type (dependency → alphabetical) ──
    sort_phase_by_dep(phase5, &edges, &mut final_order);

    // ── Phase 7: trait (dependency → alphabetical) ──
    sort_phase_by_dep(phase6, &edges, &mut final_order);

    // ── Phase 8: impl (inherent first, then trait impls; after matching type) ──
    {
        let mut placed_inherent: HashSet<usize> = HashSet::new();
        let mut placed_trait: HashSet<usize> = HashSet::new();

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
        // Group fns by visibility
        let mut pub_fns: Vec<(usize, String)> = Vec::new();
        let mut restricted_fns: Vec<(usize, String)> = Vec::new();
        let mut private_fns: Vec<(usize, String)> = Vec::new();

        for (idx, name) in phase8 {
            let vis = parsed.items[idx].visibility();
            match vis {
                Some(VisibilityTier::Pub) => pub_fns.push((idx, name)),
                Some(VisibilityTier::PubRestricted) => restricted_fns.push((idx, name)),
                _ => private_fns.push((idx, name)),
            }
        }

        // Sort each visibility group
        for group in [pub_fns, restricted_fns, private_fns] {
            sort_fn_group(&group, &edges, &mut final_order);
        }
    }

    // ── Phase 10: #[cfg(test)] mod (stable original order) ──
    final_order.extend(&phase9);

    Ok(final_order)
}

/// Helper: sort a group of fns by dependency with alphabetical tie-break.
/// `main`-first is handled by `toposort` Phase 1; this helper only groups by visibility.
fn sort_fn_group(
    group: &[(usize, String)],
    edges: &[(String, String)],
    final_order: &mut Vec<usize>,
) {
    let names: Vec<String> = group.iter().map(|(_, n)| n.clone()).collect();
    let name_to_pos: HashMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let group_edges: Vec<(String, String)> = edges
        .iter()
        .filter(|(a, b)| {
            name_to_pos.contains_key(a.as_str()) && name_to_pos.contains_key(b.as_str())
        })
        .cloned()
        .collect();

    let order = toposort(&names, &group_edges, TieBreak::Alphabetical);
    for pos in &order {
        let idx = group[*pos].0;
        final_order.push(idx);
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

        let parsed = crate::parse::parse_source(source).unwrap();
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

        let parsed = crate::parse::parse_source(source).unwrap();
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

        let parsed = crate::parse::parse_source(source).unwrap();
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

        let parsed = crate::parse::parse_source(source).unwrap();
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

        let parsed = crate::parse::parse_source(source).unwrap();
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

        let parsed = crate::parse::parse_source(source).unwrap();
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

        let parsed = crate::parse::parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();

        // c(2), b(1), a(0): callees before callers.
        assert_eq!(order, vec![2, 1, 0]);
    }
}
