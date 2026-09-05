//! Per-language reorder ordering profiles.
//!
//! A [`ReorderProfile`] is the per-language policy the reorder engine
//! consumes instead of a hard-coded item-kind table.
//!
//! It assigns every parsed item an output phase, chooses the ordering
//! strategy within each phase, ranks in-type members, and provides the
//! grammar node-kind data the reference walk matches against.

use super::toposort::TieBreak;
use ahash::AHashSet;
use derive_more::Constructor;
use rust_llm_tidy_model::parse::{ItemKind, SourceItem};

/// Per-file context for phase decisions.
///
/// Carries the engine-computed facts a profile may need beyond the item
/// itself.
pub struct PhaseContext<'a> {
    /// Macro names defined in this file. Invocations of them route to
    /// the macro phase so each follows its definition.
    pub macro_names: &'a AHashSet<&'a str>,
}

/// How the engine orders the items assigned to one phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseStrategy {
    /// Items keep their original file order.
    Stable,
    /// Items sort callers before callees; `TieBreak` orders items the
    /// edges do not constrain.
    Dependency(TieBreak),
    /// Macro uses must stay below the macro's definition (Rust
    /// `macro_rules!` textual scoping), so:
    ///
    /// - a macro invoked by another macro's body sorts first
    /// - unrelated definitions sort alphabetically
    /// - each definition is immediately followed by its invocations in
    ///   the file
    MacroDefinitions,
    /// Impl blocks order around the type they name (Rust impl blocks):
    ///
    /// - each impl sorts after the type named in its `impl` head
    ///   (`impl Foo` and `impl Trait for Foo` both name `Foo`)
    /// - `impl Foo` sorts before `impl Trait for Foo` for the same type
    /// - impls whose named type is not found in the output go last,
    ///   inherent first, source order within
    ImplsAfterTargetType,
    /// Fns group by visibility, widest first (Rust fns):
    ///
    /// - `pub` fns, then restricted (`pub(crate)` style), then private
    /// - within a group, callers sort before callees, `main` first;
    ///   unrelated fns sort alphabetically
    FnsByVisibility,
}

/// The language-specific part of reference collection: which parse-tree
/// nodes declare items, which identifier positions define names rather
/// than use them, and which node shapes record a use.
///
/// [`ReferenceCollector`] orders items by who references whom, and
/// finds references by walking a parse tree. Grammars name their nodes
/// differently, so each profile fills in these tables and the collector
/// hard-codes no language.
///
/// # Worked example
///
/// Walking `fn parse(input: &str) { helper(input) }` (simplified tree):
///
/// ```text
/// function_item            <- declares item `parse`; everything
///                             inside counts as its references
///   name: "parse"          <- defines a name, not a use: skipped
///   parameter
///     pattern: "input"     <- defines a name, not a use: skipped
///   body
///     helper(input)        <- plain identifier use; records the
///                             edge parse -> helper
/// ```
///
/// Uses are [`ReferencePosition`] entries: a bare identifier kind
/// records the node itself, path shapes record their first segment.
/// Names a grammar never produces never match.
///
/// [`ReferenceCollector`]: super::ReferenceCollector
pub struct ReferenceWalk {
    /// Node kinds that declare a top-level item (Rust: `function_item`,
    /// `struct_item`, ...). Everything inside counts as that item's
    /// references.
    pub declaration_kinds: &'static [&'static str],
    /// Spots where an identifier defines a name instead of using one:
    /// item names (`fn parse()`), bindings (`let x`, parameters),
    /// aliases (`use a as b`). Skipped, so `let helper = 1;` never
    /// references `fn helper`.
    pub decl_name_positions: &'static [DeclNamePosition],
    /// Reference-position shapes: how each reference-holding node kind
    /// records its use. Kinds the table omits are walked as pure
    /// structure: their children are examined, nothing records.
    pub reference_positions: &'static [ReferencePosition],
    /// Token kind that immediately follows a called path (Rust: `!`),
    /// marking the recorded reference as a call.
    pub macro_marker_kind: &'static str,
}

/// One definition spot: an identifier defines a name when its parent
/// node's kind is `parent_kind` and it sits in the parent's `field`.
#[derive(Constructor)]
pub struct DeclNamePosition {
    /// Node kind of the identifier's parent, e.g. `function_item`.
    pub parent_kind: &'static str,
    /// Field of the parent holding the identifier, e.g. `name`.
    pub field: &'static str,
}

/// One reference-position node kind: how the walk turns nodes of this
/// kind into a recorded use.
///
/// The walk records one referenced path per match - the node itself, or
/// the child in `path_field` - by probing the path's leftmost segment
/// against the known item names.
///
/// - `path_field`: the referenced path is this field's child; `None`
///   records the node itself.
/// - `segment_field`: the field a path-shaped node descends for its
///   leftmost segment, recursively until a bare identifier; `None` when
///   the node itself is the segment.
/// - `recurse`: whether the walk continues into the node's children
///   after recording.
pub struct ReferencePosition {
    /// Node kind that holds a reference.
    pub kind: &'static str,
    /// Field whose child is the referenced path: a call shape records
    /// the called path, a wrapped shape its wrapped type. `None` when
    /// the node itself is the path.
    pub path_field: Option<&'static str>,
    /// Field holding the leftmost segment of a path-shaped node, so
    /// `a::b::c` resolves through `path` to `a`; `None` when the node
    /// itself is the segment.
    pub segment_field: Option<&'static str>,
    /// Whether the walk recurses into the node after recording: wrapped
    /// shapes carry further references among their children, while
    /// plain paths stop after their first segment.
    pub recurse: bool,
}

/// Per-language ordering policy consumed by the reorder engine.
///
/// Implementations are stateless statics ([`Sync`]) so the engine can share
/// one profile across threads.
pub trait ReorderProfile: Sync {
    /// The output phase for a top-level `item`, given the per-file `ctx`.
    ///
    /// The engine emits phases in ascending order; phase numbers are
    /// opaque to the engine and meaningful only to this profile's
    /// [`strategy`].
    ///
    /// [`strategy`]: ReorderProfile::strategy
    fn phase(&self, item: &SourceItem, ctx: &PhaseContext<'_>) -> u32;

    /// How the engine orders the items assigned to `phase`.
    ///
    /// Consulted only for phases that hold items; [`PhaseStrategy::Stable`]
    /// is the safe default for phases a profile does not specialize.
    fn strategy(&self, phase: u32) -> PhaseStrategy;

    /// The output phase for an in-type member `kind`.
    ///
    /// Member phases share the ascending-order rule of
    /// [`phase`]. The Rust parse emits no members,
    /// so the Rust profile leaves this unused.
    ///
    /// [`phase`]: ReorderProfile::phase
    fn member_phase(&self, kind: ItemKind) -> u32;

    /// How the engine orders the members assigned to member `phase`.
    ///
    /// Members honor [`PhaseStrategy::Stable`] and
    /// [`PhaseStrategy::Dependency`]; any other strategy falls back to
    /// `Stable`.
    fn member_strategy(&self, phase: u32) -> PhaseStrategy;

    /// The grammar node-kind data for this language's reference walk.
    fn reference_walk(&self) -> &'static ReferenceWalk;
}

impl ReferencePosition {
    /// A bare identifier kind: the node itself both holds and names the
    /// reference.
    pub const fn bare(kind: &'static str) -> Self {
        Self {
            kind,
            path_field: None,
            segment_field: None,
            recurse: false,
        }
    }

    /// A path shape: the node's leftmost segment (in `segment_field`)
    /// names the reference, and the remaining segments never reference
    /// a top-level item, so the walk stops.
    pub const fn path(kind: &'static str, segment_field: &'static str) -> Self {
        Self {
            kind,
            path_field: None,
            segment_field: Some(segment_field),
            recurse: false,
        }
    }

    /// A wrapped shape: the child in `path_field` names the reference
    /// and the node's children hold further references (a generic
    /// type's type arguments), so the walk records, then recurses.
    pub const fn wrapping(kind: &'static str, path_field: &'static str) -> Self {
        Self {
            kind,
            path_field: Some(path_field),
            segment_field: Some(path_field),
            recurse: true,
        }
    }

    /// A call shape: the child in `path_field` names the called macro,
    /// and the call's arguments are never walked.
    pub const fn call(kind: &'static str, path_field: &'static str) -> Self {
        Self {
            kind,
            path_field: Some(path_field),
            segment_field: None,
            recurse: false,
        }
    }
}
