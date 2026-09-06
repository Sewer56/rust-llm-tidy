//! Ordered results for a single selected file.

use super::{Change, Diagnostic};
use std::path::PathBuf;

/// Findings, edits and processing failure for one file.
#[derive(Debug, Default)]
pub struct FileReport {
    /// Input spelling retained after alias deduplication.
    pub path: PathBuf,
    /// Changes in operation order, including previews.
    pub changes: Vec<Change>,
    /// Findings in rule execution order.
    pub diagnostics: Vec<Diagnostic>,
    /// Processing failure, separate from ordinary lint findings.
    pub failure: Option<String>,
    /// Whether all enabled phases completed with a transformation enabled.
    pub processed: bool,
}
