//! Lint and transform source with the same processing engine used by the CLI.
//!
//! # Entry points
//!
//! - [`tidy_source`]: standalone buffer processing without I/O
//! - [`run`]: file and project processing with explicit execution permissions
//! - [`config`]: configuration loading and per-file policy
//! - [`reporting`]: structured results, including partial failures
//!
//! # Lower-level operations
//!
//! - [`rules::lint`]: documentation checks and shared diagnostic codes
//! - [`rules::transform`]: tables, fences, links, ordering and visibility
//! - [`languages`]: source parsing and backend dispatch
//! - [`source`]: source items, spans and syntax-tree containers
//!
//! # Remarks
//!
//! The library does not print or exit. Inspect returned reports before choosing
//! failure policy.
//! The CLI package remains `rust-llm-tidy-cli`; it installs the `rust-llm-tidy`
//! executable.

pub use pipeline::RunOptions;
pub use pipeline::SourceOptions;
pub use pipeline::run;
pub use pipeline::tidy_source;

pub mod config;
pub mod input;
pub mod languages;
mod pipeline;
mod project;
pub mod reporting;
pub mod rules;
pub mod source;
pub mod text;
