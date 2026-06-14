//! Small, dependency-free text-tidying passes for source that has drifted
//! after LLM editing.
//!
//! Each pass rewrites a class of formatting drift in Markdown and Rust doc
//! comments in place, borrowing the input back unchanged when nothing needs
//! fixing (so every pass is idempotent).
//!
//! # Passes
//!
//! - [`fix_tables`]: realign GFM pipe tables, including those nested inside
//!   `///` and `//!` doc comments.

pub use tables::fix_tables;

pub mod tables;
