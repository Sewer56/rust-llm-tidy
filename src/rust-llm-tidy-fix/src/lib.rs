//! Small, dependency-free text-tidying passes for source that has drifted
//! after LLM editing.
//!
//! Each pass rewrites a class of formatting drift in Markdown and Rust doc
//! comments in place, borrowing the input back unchanged when nothing needs
//! fixing (so every pass is idempotent).
//!
//! # Passes
//!
//! - [`fix_fences`]: rewrite nested markdown fences to alternate backtick/tilde
//!   markers so an inner fence cannot close the outer block early; works on
//!   `.md` and `///`/`//!` doc comments.
//! - [`fix_links`]: collapse inline links `[text](url)` to reference form
//!   `[text]` plus `[text]: url` definitions; idempotent.
//! - [`fix_tables`]: realign GFM pipe tables, including those nested inside
//!   `///` and `//!` doc comments.
//!
//! [`fix_fences`] returns a [`FixOutcome`] (text + per-entity [`FixAnchor`]);
//! [`fix_links`] returns the text plus its substitution pairs; [`fix_tables`]
//! returns just the rewritten text.

pub use fences::fix_fences;
pub use links::fix_links;
pub use links::fix_links_with_min;
use std::borrow::Cow;
pub use tables::fix_tables;

pub mod fences;
pub mod links;
pub mod tables;

/// Rewritten text plus per-entity anchors returned by a fix pass.
///
/// The `text` is [`Cow::Borrowed`] when the pass changed nothing (so an
/// idempotent re-run copies zero bytes) and [`Cow::Owned`] otherwise. `anchors`
/// holds one entry per edited entity, in edit order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixOutcome<'a> {
    /// Rewritten text, borrowed back unchanged when the pass was a no-op.
    pub text: Cow<'a, str>,
    /// One anchor per edited entity, in edit order.
    pub anchors: Vec<FixAnchor>,
}

/// A per-entity edit anchor from a fix pass: a 1-based line in the pass's own
/// input and the edited entity's kind.
///
/// Anchors give a consumer one record per actual edit without re-diffing the
/// before/after text. `line` is `u32` so anchors pack tightly in the change
/// records they feed; no fix pass edits a file with more than 4 billion lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixAnchor {
    /// 1-based line (in the pass's own input) where the edited entity begins.
    pub line: u32,
    /// Kind of the edited entity.
    pub kind: FixKind,
}

/// Kind of edit a fix pass applied, used to select the record wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixKind {
    /// A nested code fence whose delimiter marker was flipped.
    Fence,
}
