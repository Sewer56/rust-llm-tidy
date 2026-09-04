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
//! - `py`/`pyi`: no AST ops, via tree-sitter-python; the parse serves the
//!   docstring dialect's text checks only.
//!
//! # Dispatch composition
//!
//! The CLI's language registry decides which ops a file may run. The AST
//! ops (`reorder`, `vis`, parser-driven `lints`) dispatch only when this
//! crate's registry also provides a backend for the extension.
//!
//! An extension without a registered backend resolves no AST ops - every
//! language except the three above - and the Python backend itself
//! carries none, so `py`/`pyi` never gain an AST op.
//!
//! The [`lexicon`] module sources the TEXT001/TEXT002 text checks for the
//! five comment-marker code families (`//`, `#`, `--`, `;`, `%`) with
//! no backend at all: a fail-closed scan of comments and the family's
//! string forms.
//!
//! The [`rust_text_regions`] module sources the same checks for `rs`
//! from the parse the backend already requires: the line-comment
//! regions plus `/** */` block docs and `#[doc = "..."]` attribute docs.
//!
//! The [`python_text_regions`] module sources them for `py`/`pyi` from
//! the Python backend's parse: first-statement triple-quoted docstrings
//! plus `#` comments.
//!
//! [`lexicon`]: self::lexicon
//! [`rust_text_regions`]: self::rust_text_regions
//! [`python_text_regions`]: self::python_text_regions
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
mod python_backend;
pub mod python_text_regions;
pub mod regions;
mod rust_backend;
pub mod rust_text_regions;
