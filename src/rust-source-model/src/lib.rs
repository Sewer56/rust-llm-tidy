//! Shared Rust source model.
//!
//! Parses a Rust source file with [`syn`] and exposes a flat list of top-level
//! [`parse::SourceItem`]s, each carrying its byte span, [`parse::ItemKind`],
//! name, visibility tier, leading doc comments, and (for functions) whether it
//! returns `Result`. Also provides atomic file I/O ([`io`]) and a line-multiset
//! safety check ([`safety`]).
//!
//! This crate holds the parsing primitives shared by the `rust-auto-reorder`
//! reordering tool and the `rust-doc-check` documentation checker. It contains
//! no reordering or checking logic of its own.

/// Atomic file I/O (tempfile + rename writes).
pub mod io;
/// Top-level item parsing, classification, and the [`parse::SourceItem`] model.
pub mod parse;
/// Line-multiset safety verification for source-preserving transforms.
pub mod safety;
