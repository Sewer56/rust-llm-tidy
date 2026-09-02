//! Per-language AST backends for `rust-llm-tidy`.
//!
//! A [`LanguageBackend`] owns one language's parse setup: the tree-sitter
//! grammar, the producer into the shared item model, and the AST ops it
//! implements. [`backend_for`] resolves a file extension to its registered
//! backend.
//!
//! Backends emit the model crate's item types
//! ([`rust_llm_tidy_model::parse::ParseResult`]), so the reorder, lint, and
//! change-record passes consume one item shape for every language.
//!
//! # Dispatch composition
//!
//! The CLI's admission registry decides which ops a file may run. The AST
//! ops (`reorder`, `vis`, parser-driven `lints`) dispatch only when this
//! crate's registry also provides a backend for the extension.
//!
//! An extension without a registered backend resolves no AST ops - today
//! every language except Rust.
//!
//! # Lookup
//!
//! [`backend_for`] matches extensions ASCII case-insensitively (`.RS`
//! resolves like `.rs`) by binary search over a sorted static table. Lookups
//! allocate nothing and run at most once per file, never per item.

pub use backend::{LanguageBackend, backend_for};
pub use rust_backend::RustBackend;

mod backend;
pub mod regions;
mod rust_backend;
