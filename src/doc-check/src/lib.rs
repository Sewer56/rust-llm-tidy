//! Documentation linter for Rust source files.
//!
//! Runs a set of documentation checks over a parsed source file and produces
//! [`Diagnostic`]s. Checks cover:
//!
//! - Missing doc comments on non-private items ([`check::missing_docs`]).
//! - Missing `# Errors` sections on public functions returning `Result`
//!   ([`check::missing_errors_section`]).
//! - `# Errors` sections whose bullets name no concrete error variant
//!   ([`check::vague_errors`]).
//! - Missing `# Arguments` sections on public functions with parameters
//!   ([`check::missing_arguments_section`]).
//! - `# Arguments` sections that do not mention every parameter name
//!   ([`check::undocumented_param`]).
//! - Placeholder text in doc comments ([`check::doc_placeholder`]).
//! - Discouraged test-function names ([`check::test_naming`]).
//!
//! # Example
//!
//! ```rust
//! use rust_doc_check::check;
//! use rust_source_model::parse;
//!
//! let source = "pub fn load() -> Result<(), String> { Ok(()) }";
//! let parsed = parse::parse_source(source).unwrap();
//! let diags = check::run_all(&parsed);
//! assert!(!diags.is_empty());
//! ```
//!
//! The checks are pure functions over a `parse::ParseResult` and produce no
//! side effects. Callers iterate the returned [`Vec<Diagnostic>`] to print or
//! filter as needed.

pub use check::run_all;
pub use diagnostic::{Diagnostic, Severity};

pub mod check;
pub mod diagnostic;
