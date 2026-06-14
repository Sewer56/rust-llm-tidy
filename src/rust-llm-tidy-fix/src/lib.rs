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
//! - [`fix_tables`]: realign GFM pipe tables, including those nested inside
//!   `///` and `//!` doc comments.

pub use fences::fix_fences;
pub use tables::fix_tables;

pub mod fences;
pub mod tables;
