//! The shared lint core for `rust-llm-tidy`.
//!
//! Holds the [`Diagnostic`] shape every pass emits, the lint-code registry
//! every language's findings draw from ([`check::LINT_CODES`]), and the
//! text rules over measured prose:
//!
//! - Oversized paragraphs (TEXT001) via [`check::run_text_checks`] and
//!   [`check::run_region_checks`].
//! - Long lines (TEXT002) through the same entry points.
//!
//! The AST item rules (DOC001-DOC006, TEST001) live with their languages:
//! each backend's `lints` module in `rust_llm_tidy-lang` implements the
//! codes over its own parse.
//!
//! # Example
//!
//! ```rust
//! use rust_llm_tidy_lint::check;
//!
//! let source = "A prose line that keeps going well past the eighty character line budget for docs.";
//! let diags = check::run_text_checks(source, "md");
//! assert!(diags.iter().any(|d| d.code == "TEXT002"));
//! ```
//!
//! The checks are pure functions over text or measured regions and produce no
//! side effects. Callers iterate the returned [`Vec<Diagnostic>`] to print or
//! filter as needed.

pub use diagnostic::{Diagnostic, Severity};

pub mod check;
pub mod diagnostic;
