//! Per-language AST backends for `rust-llm-tidy`.
//!
//! A [`LanguageBackend`] owns one language's parse setup: the tree-sitter
//! grammar, the producer into the shared item model, the lint pass, the
//! reorder composition, and the AST ops it implements.
//!
//! [`backend_for`] resolves a file extension to its registered backend.
//!
//! Backends emit the model crate's item types
//! ([`rust_llm_tidy_model::parse::ParseResult`]), so the reorder, lint, and
//! change-record passes consume one item shape for every language.
//!
//! # Registered backends
//!
//! - `rs`: every AST op (reorder, vis, lints), via the model crate's
//!   tree-sitter-rust parse.
//! - `cs`: reorder and lints, via tree-sitter-c-sharp with the C# ordering
//!   profile and XML doc-dialect checks.
//!
//! # Dispatch composition
//!
//! The CLI's admission registry decides which ops a file may run. The AST
//! ops (`reorder`, `vis`, parser-driven `lints`) dispatch only when this
//! crate's registry also provides a backend for the extension.
//!
//! An extension without a registered backend resolves no AST ops - every
//! language except the two above.
//!
//! The [`lexicon`] module sources the DOC007/DOC008 text checks for the
//! `//` and `#` comment families with no backend at all: a fail-closed
//! scan of comments and the family's string forms.
//!
//! The [`rust_text_regions`] module sources the same checks for `rs`
//! from the parse the backend already requires: the line-comment
//! regions plus `/** */` block docs and `#[doc = "..."]` attribute docs.
//!
//! [`lexicon`]: self::lexicon
//! [`rust_text_regions`]: self::rust_text_regions
//!
//! # Lookup
//!
//! [`backend_for`] matches extensions ASCII case-insensitively (`.RS`
//! resolves like `.rs`) by binary search over a sorted static table. Lookups
//! allocate nothing and run at most once per file, never per item.

pub use backend::{LanguageBackend, backend_for};
pub use rust_backend::RustBackend;

mod backend;
mod csharp;
pub mod lexicon;
pub mod regions;
mod rust_backend;
pub mod rust_text_regions;
