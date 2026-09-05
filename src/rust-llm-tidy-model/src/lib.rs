//! Shared language-neutral source model.
//!
//! Language backends emit top-level [`parse::SourceItem`]s and retain their
//! tree-sitter syntax trees in [`parse::ParseResult`].
//!
//! Each item carries its byte span, [`parse::ItemKind`], name, visibility
//! tier, leading doc comments, and (for functions) whether it returns
//! `Result`.
//!
//! Also provides atomic file I/O ([`io`]) and a line-multiset safety check
//! ([`safety`]).
//!
//! This crate holds the source data shared by the `rust-llm-tidy-reorder`
//! reordering tool and the `rust-llm-tidy-lint` documentation checker. It
//! contains no reordering or checking logic of its own.

/// Atomic file I/O (tempfile + rename writes).
pub mod io;
/// Frequency multiset of source lines for the safety check.
pub(crate) mod line_count;
/// Dominant line-ending detection (`\r\n` vs `\n`) for source-preserving
/// transforms.
pub mod line_endings;
/// Shared item types and parse-result containers.
pub mod parse;
/// Line-multiset safety verification for source-preserving transforms.
pub mod safety;
