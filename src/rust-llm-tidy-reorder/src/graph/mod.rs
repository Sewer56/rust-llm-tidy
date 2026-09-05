//! Reference-graph ordering of source files into a reading order.
//!
//! [`compute_order`] is the entry point: it collects intra-file reference
//! edges through the profile's reference walk, then orders each
//! preprocessor region's items by the per-language [`ReorderProfile`].
//!
//! Items group into phases, and each phase applies its profile strategy
//! through the stable toposort.
//!
//! [`compute_member_order`] applies the same phase machinery to the members
//! of one type body.
//!
//! The permutation puts callers before callees (and macro/impl definitions
//! before their uses).

use ahash::{AHashMap, AHashSet};
pub use collect::ReferenceCollector;
pub use profile::{
    DeclNamePosition, PhaseContext, PhaseStrategy, ReferencePosition, ReferenceWalk, ReorderProfile,
};
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, TypeMember, VisibilityTier};
use std::collections::BTreeMap;
use std::ops::Range;
pub use toposort::{TieBreak, toposort};

mod collect;
mod profile;
#[cfg(test)]
pub(crate) mod test_profiles;
mod toposort;

/// Compute the in-type member permutation for one type body.
///
/// Members order within their preprocessor regions exactly like top-level
/// items: consecutive runs of equal [`TypeMember::region`] emit in
/// original order.
///
/// Inside a run, members bucket by [`ReorderProfile::member_phase`] with
/// each phase applying [`ReorderProfile::member_strategy`]. Member phases
/// honor [`PhaseStrategy::Stable`] and [`PhaseStrategy::Dependency`];
/// other strategies fall back to `Stable`.
///
/// Returns a permutation of `0..members.len()` (identity when nothing
/// moves).
///
/// # Arguments
///
/// - `members` - the type's members in source order.
/// - `edges` - reference dependency edges as member positions
///   (`(referencer, referenced)`), for caller-first ordering.
/// - `profile` - the language's ordering policy.
pub fn compute_member_order(
    members: &[TypeMember],
    edges: &[(usize, usize)],
    profile: &dyn ReorderProfile,
) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::with_capacity(members.len());

    // Region runs: members never cross a conditional boundary.
    let mut run_start = 0;
    while run_start < members.len() {
        let region = members[run_start].region();
        let mut run_end = run_start + 1;
        while run_end < members.len() && members[run_end].region() == region {
            run_end += 1;
        }

        // Phase buckets preserve source order within a bucket.
        let mut buckets: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (offset, member) in members[run_start..run_end].iter().enumerate() {
            let phase = profile.member_phase(*member.kind());
            buckets.entry(phase).or_default().push(run_start + offset);
        }

        for (phase, group) in &buckets {
            match profile.member_strategy(*phase) {
                PhaseStrategy::Dependency(tie_break) => {
                    let names: Vec<&str> = group
                        .iter()
                        .map(|&pos| members[pos].name().unwrap_or(""))
                        .collect();
                    let order = toposort_positions(&names, group, edges, tie_break);
                    out.extend(order.into_iter().map(|pos| group[pos]));
                }
                // Members only honor Stable and Dependency; anything else
                // orders stably.
                _ => out.extend_from_slice(group),
            }
        }
        run_start = run_end;
    }

    out
}

/// Extract reference edges from parsed source and compute a topological
/// ordering.
///
/// Items order within their preprocessor regions: items partition into
/// consecutive runs of equal [`SourceItem::region`], and the runs emit in
/// original order - so no item ever moves across a conditional boundary.
///
/// Each run orders by the profile's phases and strategies. Sources without
/// preprocessor conditionals carry region `0` on every item, making the
/// whole file one run.
///
/// [`SourceItem::region`]: `rust_llm_tidy_model::parse::SourceItem::region`
///
/// Phases, their ordering strategies, and the reference walk all come
/// from `profile`.
///
/// Returns a `Vec<usize>` suitable for constructing a `Permutation` in the
/// reorder stage. Each element is an index into `parsed.items`.
///
/// # Arguments
///
/// - `parsed` - the parsed source whose top-level items are ordered.
/// - `profile` - the language's ordering policy.
///
/// # Errors
///
/// Returns an [`anyhow::Error`] on internal graph-ordering failure. Because
/// ordering operates over an already-parsed [`ParseResult`] and never
/// re-parses, this never fires for a well-formed `parsed`.
pub fn compute_order(
    parsed: &ParseResult,
    profile: &dyn ReorderProfile,
) -> anyhow::Result<Vec<usize>> {
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
    //        parse result instead of re-parsing `parsed.source`. The walk's
    //        node-kind matching comes from the profile. ──
    let tree = parsed.syntax_tree();
    let mut collector =
        ReferenceCollector::new(name_to_idx, macro_names.clone(), profile.reference_walk());
    collector.collect(tree, parsed.source.as_bytes());
    let edges = collector.into_edges();

    // ── 3. Order each preprocessor region run; runs emit in original order
    //        so no item crosses a conditional boundary. ──
    let ctx = PhaseContext {
        macro_names: &macro_names,
    };
    let mut final_order: Vec<usize> = Vec::with_capacity(n);
    let mut run_start = 0;
    while run_start < n {
        let region = parsed.items[run_start].region();
        let mut run_end = run_start + 1;
        while run_end < n && parsed.items[run_end].region() == region {
            run_end += 1;
        }
        order_region(
            parsed,
            profile,
            &ctx,
            &edges,
            run_start..run_end,
            &mut final_order,
        );
        run_start = run_end;
    }

    Ok(final_order)
}

/// Order the items of one region run into `out`.
///
/// Items bucket by profile phase (push order preserves source order within
/// a bucket); buckets emit in ascending phase, each applying its profile
/// strategy.
fn order_region(
    parsed: &ParseResult,
    profile: &dyn ReorderProfile,
    ctx: &PhaseContext<'_>,
    edges: &[(usize, usize)],
    run: Range<usize>,
    out: &mut Vec<usize>,
) {
    let mut buckets: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for idx in run {
        let phase = profile.phase(&parsed.items[idx], ctx);
        buckets.entry(phase).or_default().push(idx);
    }

    for (phase, items) in &buckets {
        match profile.strategy(*phase) {
            PhaseStrategy::Stable => out.extend_from_slice(items),
            PhaseStrategy::Dependency(tie_break) => {
                out.extend(dependency_order(parsed, items, edges, tie_break));
            }
            PhaseStrategy::MacroDefinitions => {
                emit_macro_definitions(parsed, items, edges, out);
            }
            PhaseStrategy::ImplsAfterTargetType => {
                emit_impls_after_target_type(parsed, items, out);
            }
            PhaseStrategy::FnsByVisibility => {
                emit_fns_by_visibility(parsed, items, edges, out);
            }
        }
    }
}

/// Emit one fn phase: visibility groups (`pub`, then restricted, then
/// private), each dependency-sorted with an alphabetical tie-break.
/// `main`-first is handled by the toposort itself.
fn emit_fns_by_visibility(
    parsed: &ParseResult,
    items: &[usize],
    edges: &[(usize, usize)],
    out: &mut Vec<usize>,
) {
    let mut pub_fns: Vec<usize> = Vec::new();
    let mut restricted_fns: Vec<usize> = Vec::new();
    let mut private_fns: Vec<usize> = Vec::new();

    for &idx in items {
        match parsed.items[idx].visibility() {
            Some(VisibilityTier::Pub) => pub_fns.push(idx),
            Some(VisibilityTier::PubRestricted) => restricted_fns.push(idx),
            _ => private_fns.push(idx),
        }
    }

    for group in [pub_fns, restricted_fns, private_fns] {
        out.extend(dependency_order(
            parsed,
            &group,
            edges,
            TieBreak::Alphabetical,
        ));
    }
}

/// Emit one impl phase: inherent impls after their matching type, then
/// trait impls after their matching type (and after inherent impls for the
/// same type); orphan impls follow, inherent first, stable within.
fn emit_impls_after_target_type(parsed: &ParseResult, items: &[usize], out: &mut Vec<usize>) {
    // Split inherent from trait impls; both keep source order.
    let mut inherent: Vec<usize> = Vec::new();
    let mut trait_impls: Vec<usize> = Vec::new();
    for &idx in items {
        if parsed.items[idx].is_trait_impl() {
            trait_impls.push(idx);
        } else {
            inherent.push(idx);
        }
    }

    let mut placed_inherent: AHashSet<usize> = AHashSet::new();
    let mut placed_trait: AHashSet<usize> = AHashSet::new();

    // Place inherent impls after their matching type. The loop bound is
    // fixed before any push, so pushed impls do not trigger re-scans.
    for i in 0..out.len() {
        let type_idx = out[i];
        if let Some(type_name) = parsed.items[type_idx].name() {
            for &impl_idx in &inherent {
                if placed_inherent.contains(&impl_idx) {
                    continue;
                }
                if let Some(impl_target) = parsed.items[impl_idx].impl_target_name()
                    && impl_target == type_name
                {
                    out.push(impl_idx);
                    placed_inherent.insert(impl_idx);
                }
            }
        }
    }

    // Place trait impls after their matching type (and after inherent
    // impls for same type).
    for i in 0..out.len() {
        let type_idx = out[i];
        if let Some(type_name) = parsed.items[type_idx].name() {
            for &impl_idx in &trait_impls {
                if placed_trait.contains(&impl_idx) {
                    continue;
                }
                if let Some(impl_target) = parsed.items[impl_idx].impl_target_name()
                    && impl_target == type_name
                {
                    out.push(impl_idx);
                    placed_trait.insert(impl_idx);
                }
            }
        }
    }

    // Orphan impls: inherent first, then trait, stable order within.
    for &impl_idx in &inherent {
        if !placed_inherent.contains(&impl_idx) {
            out.push(impl_idx);
        }
    }
    for &impl_idx in &trait_impls {
        if !placed_trait.contains(&impl_idx) {
            out.push(impl_idx);
        }
    }
}

/// Emit one macro phase: definitions dependency-sorted alphabetically, each
/// immediately followed by its local invocations in source order, so a
/// `macro_rules!` definition always precedes its use sites (Rust
/// `macro_rules!` uses textual scoping).
fn emit_macro_definitions(
    parsed: &ParseResult,
    items: &[usize],
    edges: &[(usize, usize)],
    out: &mut Vec<usize>,
) {
    // Split definitions from invocations; both keep source order.
    let mut defs: Vec<usize> = Vec::new();
    let mut invocations: Vec<usize> = Vec::new();
    for &idx in items {
        match parsed.items[idx].kind() {
            ItemKind::Macro => defs.push(idx),
            ItemKind::MacroInvocation => invocations.push(idx),
            // Other kinds in a macro phase keep source order, leading the
            // phase.
            _ => out.push(idx),
        }
    }

    let def_order = dependency_order(parsed, &defs, edges, TieBreak::Alphabetical);

    // Group invocations by macro name. Invocations whose name has no
    // matching definition emit last in source order (unreachable when
    // the profile routes an invocation to this phase only when a local
    // definition shares its name).
    let def_names: AHashSet<&str> = def_order
        .iter()
        .filter_map(|&idx| parsed.items[idx].name())
        .filter(|name| !name.is_empty())
        .collect();
    let mut invocations_by_def: AHashMap<&str, Vec<usize>> = AHashMap::new();
    let mut orphans: Vec<usize> = Vec::new();
    for &inv_idx in &invocations {
        let inv_name = parsed.items[inv_idx].name().unwrap_or("");
        if def_names.contains(inv_name) {
            invocations_by_def
                .entry(inv_name)
                .or_default()
                .push(inv_idx);
        } else {
            orphans.push(inv_idx);
        }
    }

    for &def_idx in &def_order {
        out.push(def_idx);
        let def_name = parsed.items[def_idx].name().unwrap_or("");
        // `remove` (not `get`): duplicate definition names - legal in
        // Rust, a later `macro_rules!` shadows the earlier - would attach
        // the shared invocations once per definition and repeat indices.
        if let Some(invocs) = invocations_by_def.remove(def_name) {
            out.extend(invocs);
        }
    }
    out.extend(&orphans);
}

/// Sort one group's items by reference dependency with `tie_break` for
/// unconstrained items. Names are borrowed from `parsed.items[group[*]]`,
/// so no per-item name clone is needed.
fn dependency_order(
    parsed: &ParseResult,
    group: &[usize],
    edges: &[(usize, usize)],
    tie_break: TieBreak,
) -> Vec<usize> {
    let names: Vec<&str> = group
        .iter()
        .map(|&idx| parsed.items[idx].name().unwrap_or(""))
        .collect();
    let order = toposort_positions(&names, group, edges, tie_break);
    order.into_iter().map(|pos| group[pos]).collect()
}

/// Topologically sort `group` (item or member positions) by `edges`.
///
/// [`toposort`] speaks in positions local to one call (`0..group.len()`),
/// while callers hold node ids from a wider numbering: item indices into
/// `parsed.items` for top-level items, or member positions for one type
/// body.
///
/// This adapter bridges the two:
///
/// 1. Map each `group` value to its 0-based position within the group.
/// 2. Keep only `edges` whose `(referencer, referenced)` endpoints both
///    lie in `group`, rewriting each pair to those positions; edges
///    touching anything outside the group never constrain the sort.
/// 3. Delegate to [`toposort`] with the group-local names and edges.
///
/// Returns a permutation where `order[i]` is the position in `group` of
/// the i-th entry in reading order; callers map back with
/// `group[order[i]]`.
///
/// # Arguments
///
/// - `names` - `names[i]` is the name of `group[i]`; `toposort` reads
///   names only for tie-breaking and `main`-first seeding.
/// - `group` - the nodes to sort, in source order.
/// - `edges` - `(referencer, referenced)` pairs in the same numbering as
///   `group`'s values; pairs may span the whole file or type body.
/// - `tie_break` - ordering for unconstrained and cyclic nodes.
///
/// # Worked example
///
/// One fn phase bucket:
///
/// ```text
/// names  = ["parse", "run", "emit"]
/// group  = [0, 1, 5]                // item indices, source order
/// edges  = [(1, 0), (0, 5), (2, 7)] // file-wide pairs
/// ```
///
/// Edge `(1, 0)` says `run` references `parse`; `(0, 5)` says `parse`
/// references `emit`. Item 7 is not in `group`, so `(2, 7)` drops.
///
/// Rewritten to group positions the edges become `[(1, 0), (0, 2)]` and
/// the returned permutation is `[1, 0, 2]`: reading order run, parse,
/// emit.
///
/// [`toposort`]: fn@toposort
fn toposort_positions(
    names: &[&str],
    group: &[usize],
    edges: &[(usize, usize)],
    tie_break: TieBreak,
) -> Vec<usize> {
    // Group-local position for each node id the group holds.
    let mut pos_by_node: AHashMap<usize, usize> = AHashMap::with_capacity(group.len());
    for (pos, &node) in group.iter().enumerate() {
        pos_by_node.insert(node, pos);
    }

    let mut group_edges: Vec<(usize, usize)> = Vec::new();
    for &(referencer, referenced) in edges {
        if let (Some(&from), Some(&to)) =
            (pos_by_node.get(&referencer), pos_by_node.get(&referenced))
        {
            group_edges.push((from, to));
        }
    }

    toposort(names, &group_edges, tie_break)
}

#[cfg(test)]
mod tests {
    use super::test_profiles::{CallersFirstProfile, MembersFirstProfile};
    use super::*;
    use rust_llm_tidy_lang::{LanguageBackend, RustBackend};

    /// Items only reorder within their preprocessor region run: a caller in
    /// region 0 with its callee also in region 0 still reorders, while a
    /// region-1 item sitting between them never crosses.
    #[test]
    fn items_reorder_only_within_region_runs() {
        // Source order: fn z(0, region 1) splits two region-0 fns where
        // caller b precedes callee a by reorder.
        let source = "fn b() { a(); }\nfn z() {}\nfn a() {}\n";
        let parsed = RustBackend.parse(source).unwrap();
        let items: Vec<_> = parsed
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                // b -> region 0, z -> region 1, a -> region 2: three runs,
                // so nothing can move.
                item.clone().with_region(i as u32)
            })
            .collect();
        let tree = parsed.syntax_tree().clone();
        let regioned = ParseResult::new(
            items,
            parsed.source.clone(),
            tree,
            parsed.preamble_end,
            parsed.trailer_start,
        );

        let order = compute_order(&regioned, &CallersFirstProfile).unwrap();

        // Each item is its own region run: identity, even though b calls a.
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// A region split between two fns still allows each region's fns to
    /// reorder among themselves.
    #[test]
    fn region_runs_reorder_independently() {
        // Region 0: fn b calls fn a (b first after reorder).
        // Region 1: fn d calls fn c (d first after reorder).
        let source = "fn a() {}\nfn b() { a(); }\nfn c() {}\nfn d() { c(); }\n";
        let parsed = RustBackend.parse(source).unwrap();
        let items: Vec<_> = parsed
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| item.clone().with_region(if i < 2 { 0 } else { 1 }))
            .collect();
        let tree = parsed.syntax_tree().clone();
        let regioned = ParseResult::new(
            items,
            parsed.source.clone(),
            tree,
            parsed.preamble_end,
            parsed.trailer_start,
        );

        let order = compute_order(&regioned, &CallersFirstProfile).unwrap();

        // b before a (pos 1 before 0), d before c (pos 3 before 2); the
        // region 0/1 boundary keeps run 0's items before run 1's.
        assert_eq!(order, vec![1, 0, 3, 2]);
    }

    /// Members order by member phase, and method edges order callers first
    /// within the method phase.
    #[test]
    fn members_order_by_phase_then_caller_first() {
        let members = [
            TypeMember::new(0, 10, 0, ItemKind::Fn, Some("calls_helper".into())),
            TypeMember::new(10, 20, 0, ItemKind::Const, Some("Field".into())),
            TypeMember::new(20, 30, 0, ItemKind::Fn, Some("helper".into())),
        ];
        // calls_helper (0) calls helper (2).
        let edges = vec![(0usize, 2usize)];

        let order = compute_member_order(&members, &edges, &MembersFirstProfile);

        // Field (phase 0) first, then callers before callees among methods.
        assert_eq!(order, vec![1, 0, 2]);
    }

    /// Unconstrained members of one Dependency phase keep source order (the
    /// profile's stable tie-break), not alphabetical order.
    #[test]
    fn unordered_members_keep_source_order() {
        let members = [
            TypeMember::new(0, 10, 0, ItemKind::Fn, Some("zeta".into())),
            TypeMember::new(10, 20, 0, ItemKind::Fn, Some("alpha".into())),
        ];

        let order = compute_member_order(&members, &[], &MembersFirstProfile);

        assert_eq!(
            order,
            vec![0, 1],
            "no edges: source order, not alphabetical"
        );
    }

    /// Members never cross a preprocessor region boundary, even when edges
    /// and phases would move them.
    #[test]
    fn members_never_cross_region_runs() {
        let members = [
            TypeMember::new(0, 10, 0, ItemKind::Fn, Some("caller".into())),
            TypeMember::new(10, 20, 1, ItemKind::Const, Some("Field".into())),
            TypeMember::new(20, 30, 2, ItemKind::Fn, Some("callee".into())),
        ];
        // caller (0) calls callee (2), but they sit in different regions.
        let edges = vec![(0usize, 2usize)];

        let order = compute_member_order(&members, &edges, &MembersFirstProfile);

        // Three singleton runs: identity despite the edge and the phase
        // pull toward fields-first.
        assert_eq!(order, vec![0, 1, 2]);
    }
}
