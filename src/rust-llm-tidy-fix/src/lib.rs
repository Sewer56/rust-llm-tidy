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
//! - [`fix_links`]: hoist repeated inline links `[text](url)` to reference
//!   definitions `[text]` plus a trailing `[text]: url` block; idempotent.
//! - [`fix_tables`]: realign GFM pipe tables, including those nested inside
//!   `///` and `//!` doc comments.
//!
//! [`fix_tables`]/[`fix_fences`] return a [`FixOutcome`] (text + per-entity
//! [`FixAnchor`]); [`fix_links`] returns the text plus its before/after pairs.

pub use fences::fix_fences;
pub use links::fix_links;
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
/// input, the edited entity's kind, and an optional name.
///
/// Anchors give a consumer one record per actual edit without re-diffing the
/// before/after text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixAnchor {
    /// 1-based line (in the pass's own input) where the edited entity begins.
    pub line: usize,
    /// Kind of the edited entity.
    pub kind: FixKind,
    /// Name of the edited entity (e.g. a hoisted link's text), when it has one.
    pub name: Option<String>,
}

/// Kind of edit a fix pass applied, used to select the record wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixKind {
    /// A GFM pipe table that was realigned.
    Table,
    /// A nested code fence whose delimiter marker was flipped.
    Fence,
}
