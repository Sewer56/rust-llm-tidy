//! Benchmark fixture: synthetic reference-only doc module.
//! doc tier; no-op for fix_links (every inline link is already reference-style
//! `[text]` with an in-comment definition, so none is eligible and fix_links
//! borrows the input back unchanged). Exercises the borrowed fast path over
//! bracket-heavy doc comments.
//! Synthetic, not sourced from a project.

//! Reference-only module documentation for a config builder.
//!
//! See [`Builder`] and [`Config`] for the core types.
//!
//! [`Builder`]: crate::Builder
//! [`Config`]: crate::Config

/// Builds a [`Config`] from parts.
///
/// [`Config`]: crate::Config
pub struct Builder;

/// The assembled [`Config`] value.
///
/// [`Config`]: crate::Config
pub struct Config;
