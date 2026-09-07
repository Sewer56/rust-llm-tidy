//! Explicit permissions and rule selection for file processing.

use std::path::PathBuf;

/// Select files and processing behavior without enabling side effects by default.
///
/// Empty paths process nothing unless `git_changed` is enabled.
#[derive(Debug, Default, Clone)]
pub struct RunOptions {
    /// Files or directories to process, expanded using the effective extensions.
    pub paths: Vec<PathBuf>,
    /// Write changes to files instead of previewing them.
    pub apply: bool,
    /// Query tracked Git changes when `paths` is empty.
    pub git_changed: bool,
    /// Permit Cargo subprocesses for Rust project discovery.
    /// Without permission, visibility uses only standalone file facts.
    pub cargo_discovery: bool,
    /// Permit configured subprocesses after processing; ignored during preview.
    pub post_process: bool,
    /// Rule or operation whitelist overriding configuration when nonempty.
    pub include: Vec<String>,
    /// Extra rule or operation exclusions.
    pub exclude: Vec<String>,
    /// Extra admitted extensions without leading dots.
    pub extensions: Vec<String>,
}
