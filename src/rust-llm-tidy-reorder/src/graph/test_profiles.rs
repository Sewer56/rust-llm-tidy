//! Test-only [`ReorderProfile`] implementations shared by the graph and
//! reorder-stage test suites; compiled only under `#[cfg(test)]`.
//!
//! [`ReorderProfile`]: super::profile::ReorderProfile

use super::profile::{
    DeclNamePosition, PhaseContext, PhaseStrategy, ReferencePosition, ReferenceWalk, ReorderProfile,
};
use super::toposort::TieBreak;
use rust_llm_tidy_model::parse::{ItemKind, SourceItem};

/// A minimal walk for parsed Rust fixtures: fns declare items and bare
/// identifiers reference them.
static CALLERS_FIRST_WALK: ReferenceWalk = ReferenceWalk {
    declaration_kinds: &["function_item"],
    decl_name_positions: &[DeclNamePosition::new("function_item", "name")],
    reference_positions: &[ReferencePosition::bare("identifier")],
    macro_marker_kind: "!",
};
/// A walk that records nothing: the member tests hand their edges over
/// directly, so this profile's walk never runs.
static NO_REFERENCES: ReferenceWalk = ReferenceWalk {
    declaration_kinds: &[],
    decl_name_positions: &[],
    reference_positions: &[],
    macro_marker_kind: "",
};

/// A caller-first profile: every top-level item shares one dependency
/// phase with a stable tie-break, so items order callers before callees.
pub(crate) struct CallersFirstProfile;

/// A member-ordering profile: fields (`Const`) lead, methods (`Fn`)
/// follow with caller-first edges and a stable tie-break.
pub(crate) struct MembersFirstProfile;

impl ReorderProfile for CallersFirstProfile {
    fn phase(&self, _item: &SourceItem, _ctx: &PhaseContext<'_>) -> u32 {
        0
    }

    fn strategy(&self, _phase: u32) -> PhaseStrategy {
        PhaseStrategy::Dependency(TieBreak::Stable)
    }

    fn member_phase(&self, _kind: ItemKind) -> u32 {
        0
    }

    fn member_strategy(&self, _phase: u32) -> PhaseStrategy {
        PhaseStrategy::Stable
    }

    fn reference_walk(&self) -> &'static ReferenceWalk {
        &CALLERS_FIRST_WALK
    }
}

impl ReorderProfile for MembersFirstProfile {
    fn phase(&self, _item: &SourceItem, _ctx: &PhaseContext<'_>) -> u32 {
        0
    }

    fn strategy(&self, _phase: u32) -> PhaseStrategy {
        PhaseStrategy::Stable
    }

    fn member_phase(&self, kind: ItemKind) -> u32 {
        match kind {
            ItemKind::Const => 0,
            ItemKind::Fn => 1,
            _ => 2,
        }
    }

    fn member_strategy(&self, phase: u32) -> PhaseStrategy {
        match phase {
            1 => PhaseStrategy::Dependency(TieBreak::Stable),
            _ => PhaseStrategy::Stable,
        }
    }

    fn reference_walk(&self) -> &'static ReferenceWalk {
        &NO_REFERENCES
    }
}
