//! Test-only [`ReorderProfile`] implementations shared by the graph and
//! reorder-stage test suites; compiled only under `#[cfg(test)]`.
//!
//! [`ReorderProfile`]: super::profile::ReorderProfile

use super::profile::{PhaseContext, PhaseStrategy, ReferenceWalk, ReorderProfile};
use super::rust_profile::RustProfile;
use super::toposort::TieBreak;
use rust_llm_tidy_model::parse::ItemKind;

/// A member-ordering profile: fields (`Const`) lead, methods (`Fn`)
/// follow with caller-first edges and a stable tie-break.
pub(crate) struct MembersFirstProfile;

impl ReorderProfile for MembersFirstProfile {
    fn phase(
        &self,
        _item: &rust_llm_tidy_model::parse::SourceItem,
        _ctx: &PhaseContext<'_>,
    ) -> u32 {
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
        RustProfile.reference_walk()
    }
}
