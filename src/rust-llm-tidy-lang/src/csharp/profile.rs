//! The C# reorder profile: the ordering policy the engine consumes.
//!
//! Top level, `using` directives pin first and keep their relative order
//! (they never reorder among themselves); every other top-level item -
//! namespaces, types, preprocessor directives, statements - keeps its
//! source order.
//!
//! Hoisting a `using` only widens the names it introduces to earlier
//! lines, so the pin is semantically safe; nothing else moves.
//!
//! Inside a type body, members order by the Rider/ReSharper default
//! buckets, each stable except methods:
//!
//! 1. fields
//! 2. constructors
//! 3. finalizers
//! 4. delegates and events
//! 5. enums and nested types
//! 6. properties and indexers
//! 7. operators
//! 8. methods, callers before callees (stable tie-break)
//!
//! Inside a namespace body the same table applies with `using` directives
//! pinned first, so nested usings hoist above the namespace's types.

use rust_llm_tidy_model::parse::{ItemKind, SourceItem, TypeMember};
use rust_llm_tidy_reorder::graph::{
    DeclNamePosition, PhaseContext, PhaseStrategy, ReferenceWalk, ReorderProfile, TieBreak,
};
use std::collections::HashMap;

/// The C# grammar's declaration node kinds, for the engine's reference walk
/// over top-level items.
static CSHARP_REFERENCE_WALK: ReferenceWalk = ReferenceWalk {
    declaration_kinds: &[
        "class_declaration",
        "struct_declaration",
        "interface_declaration",
        "record_declaration",
        "enum_declaration",
        "delegate_declaration",
    ],
    decl_name_positions: DECL_NAME_POSITIONS,
};
/// The stable phase of every non-`using` item and of unrecognized members.
const STABLE_PHASE: u32 = 2;
/// The pinned phase of `using` directives.
const USING_PHASE: u32 = 1;
/// C# declaration-name positions that reference walks must not record as
/// references.
static DECL_NAME_POSITIONS: &[DeclNamePosition] = &[
    DeclNamePosition::new("class_declaration", "name"),
    DeclNamePosition::new("struct_declaration", "name"),
    DeclNamePosition::new("interface_declaration", "name"),
    DeclNamePosition::new("record_declaration", "name"),
    DeclNamePosition::new("enum_declaration", "name"),
    DeclNamePosition::new("enum_member_declaration", "name"),
    DeclNamePosition::new("delegate_declaration", "name"),
    DeclNamePosition::new("namespace_declaration", "name"),
    DeclNamePosition::new("file_scoped_namespace_declaration", "name"),
    DeclNamePosition::new("method_declaration", "name"),
    DeclNamePosition::new("property_declaration", "name"),
    DeclNamePosition::new("event_declaration", "name"),
    DeclNamePosition::new("variable_declarator", "name"),
    DeclNamePosition::new("parameter", "name"),
];

/// The C# reorder profile: `using` directives pinned first, everything else
/// in source order at the top level, and the documented member buckets
/// inside type and namespace bodies.
pub(super) struct CSharpProfile;

impl ReorderProfile for CSharpProfile {
    fn phase(&self, item: &SourceItem, _ctx: &PhaseContext<'_>) -> u32 {
        match item.kind() {
            ItemKind::Using => USING_PHASE,
            _ => STABLE_PHASE,
        }
    }

    fn strategy(&self, _phase: u32) -> PhaseStrategy {
        PhaseStrategy::Stable
    }

    fn member_phase(&self, kind: ItemKind) -> u32 {
        match kind {
            ItemKind::Using => USING_PHASE,
            ItemKind::Const | ItemKind::Static => 1,
            ItemKind::Constructor => 2,
            ItemKind::Destructor => 3,
            ItemKind::Delegate | ItemKind::Event => 4,
            ItemKind::Enum
            | ItemKind::Class
            | ItemKind::Struct
            | ItemKind::Interface
            | ItemKind::Record
            | ItemKind::Namespace
            | ItemKind::Other => 5,
            ItemKind::Property => 6,
            ItemKind::Operator => 7,
            ItemKind::Fn => 8,
            _ => STABLE_PHASE,
        }
    }

    fn member_strategy(&self, phase: u32) -> PhaseStrategy {
        match phase {
            8 => PhaseStrategy::Dependency(TieBreak::Stable),
            _ => PhaseStrategy::Stable,
        }
    }

    fn reference_walk(&self) -> &'static ReferenceWalk {
        &CSHARP_REFERENCE_WALK
    }
}

/// Caller-first edges among the members of one type or namespace body.
///
/// `decls` are the body's declaration nodes in source order, aligned
/// 1:1 with `members` (the parse builds both from the same body walk).
///
/// Every member's subtree is scanned for `identifier` references; a
/// reference whose text names a sibling member records a
/// `(referencer, referenced)` edge in member positions.
///
/// Declaration-name positions do not record; the trailing segment of a
/// member access (`this.Helper`) does.
///
/// The name map and probe buffer are reused across members, so the scan
/// allocates nothing per identifier.
pub(super) fn member_edges(
    decls: &[tree_sitter::Node<'_>],
    members: &[TypeMember],
    source: &str,
) -> Vec<(usize, usize)> {
    let mut name_to_idx: HashMap<&str, usize> = HashMap::with_capacity(members.len());
    for (idx, member) in members.iter().enumerate() {
        if let Some(name) = member.name() {
            name_to_idx.entry(name).or_insert(idx);
        }
    }

    let mut edges = Vec::new();
    let mut scratch = String::new();
    let Some(first) = decls.first().copied() else {
        return edges;
    };
    // `decls` and `members` align 1:1 (the parse built both from the same
    // body walk), so positions translate directly. One cursor is reused
    // across every member's subtree: a fresh cursor per node would
    // allocate behind every step.
    let mut cursor = first.walk();
    for (idx, decl) in decls.iter().enumerate() {
        record_references(
            *decl,
            source,
            &name_to_idx,
            &mut scratch,
            idx,
            &mut edges,
            &mut cursor,
        );
    }
    edges
}

/// Depth-first scan of one member's subtree, recording an edge for every
/// identifier that names a sibling member.
///
/// `cursor` is reset to `node` and walked with first-child/next-sibling
/// moves until the walk returns to `node`, so the scan allocates nothing
/// per node.
fn record_references<'t>(
    node: tree_sitter::Node<'t>,
    source: &str,
    name_to_idx: &HashMap<&str, usize>,
    scratch: &mut String,
    referencer: usize,
    edges: &mut Vec<(usize, usize)>,
    cursor: &mut tree_sitter::TreeCursor<'t>,
) {
    cursor.reset(node);
    'walk: loop {
        probe_identifier(
            cursor.node(),
            source,
            name_to_idx,
            scratch,
            referencer,
            edges,
        );
        if cursor.goto_first_child() {
            continue 'walk;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == node {
                break 'walk;
            }
        }
    }
}

/// Probe one node as a reference identifier, recording an edge when it
/// names a sibling member of `referencer`.
fn probe_identifier(
    node: tree_sitter::Node<'_>,
    source: &str,
    name_to_idx: &HashMap<&str, usize>,
    scratch: &mut String,
    referencer: usize,
    edges: &mut Vec<(usize, usize)>,
) {
    if node.kind() != "identifier" || is_decl_name_position(node) {
        return;
    }
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return;
    };
    scratch.clear();
    scratch.push_str(text);
    if let Some(&target) = name_to_idx.get(scratch.as_str())
        && target != referencer
    {
        edges.push((referencer, target));
    }
}

/// True when `node` (an `identifier`) is the `name` field of a declaration
/// parent, so it names rather than references.
fn is_decl_name_position(node: tree_sitter::Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let Some(field) = parent_field_name(parent, node) else {
        return false;
    };
    DECL_NAME_POSITIONS
        .iter()
        .any(|pos| parent.kind() == pos.parent_kind && field == pos.field)
}

/// The field name of `node` within its parent, if any.
fn parent_field_name(
    parent: tree_sitter::Node<'_>,
    node: tree_sitter::Node<'_>,
) -> Option<&'static str> {
    for i in 0..parent.child_count() {
        if parent.child(i as u32) == Some(node) {
            return parent.field_name_for_child(i as u32);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_reorder::graph::PhaseStrategy;

    /// Every member kind maps to its documented bucket: fields, then
    /// constructors, finalizers, delegates/events, enums and nested types,
    /// properties, operators, methods; usings pin first for namespace
    /// bodies.
    #[test]
    fn member_phases_follow_the_documented_buckets() {
        let cases = [
            (ItemKind::Const, 1),
            (ItemKind::Static, 1),
            (ItemKind::Constructor, 2),
            (ItemKind::Destructor, 3),
            (ItemKind::Delegate, 4),
            (ItemKind::Event, 4),
            (ItemKind::Enum, 5),
            (ItemKind::Class, 5),
            (ItemKind::Struct, 5),
            (ItemKind::Interface, 5),
            (ItemKind::Record, 5),
            (ItemKind::Property, 6),
            (ItemKind::Operator, 7),
            (ItemKind::Fn, 8),
            (ItemKind::Using, 1),
        ];
        for (kind, phase) in cases {
            assert_eq!(
                CSharpProfile.member_phase(kind),
                phase,
                "{kind} must map to phase {phase}"
            );
        }
    }

    /// Only the method bucket dependency-sorts (callers before callees);
    /// every other bucket stays stable.
    #[test]
    fn only_methods_dependency_sort() {
        for phase in 0..=9 {
            let expected = if phase == 8 {
                PhaseStrategy::Dependency(TieBreak::Stable)
            } else {
                PhaseStrategy::Stable
            };
            assert_eq!(
                CSharpProfile.member_strategy(phase),
                expected,
                "phase {phase}"
            );
        }
    }

    /// Top level, usings pin first and everything else shares one stable
    /// phase; no top-level phase ever dependency-sorts.
    #[test]
    fn top_level_usings_pin_first_and_everything_else_is_stable() {
        let parsed = super::super::parse::parse("using System;\nclass C { }\n").unwrap();
        let ctx = PhaseContext {
            macro_names: &Default::default(),
        };
        assert_eq!(CSharpProfile.phase(&parsed.items[0], &ctx), USING_PHASE);
        assert_eq!(CSharpProfile.phase(&parsed.items[1], &ctx), STABLE_PHASE);
        assert_eq!(
            CSharpProfile.strategy(USING_PHASE),
            PhaseStrategy::Stable,
            "usings never reorder among themselves"
        );
    }
}
