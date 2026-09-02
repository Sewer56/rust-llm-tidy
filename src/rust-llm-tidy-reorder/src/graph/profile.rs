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
/// nodes declare items, and which identifier positions define names
/// rather than use them.
///
/// [`ReferenceCollector`] orders items by who references whom, and
/// finds references by walking a parse tree. Grammars name their nodes
/// differently, so each profile fills in this table and the collector
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
/// Uses need no entries: identifier uses look the same in every
/// supported grammar. Names a grammar never produces never match.
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
