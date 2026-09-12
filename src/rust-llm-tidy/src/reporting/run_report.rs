//! Complete processing results, including partial failures.

use super::{FileReport, Severity};
use anyhow::bail;
use std::path::PathBuf;

/// Results returned without printing or selecting a process exit code.
#[derive(Debug, Default)]
pub struct RunReport {
    /// File results in deterministic input order.
    pub files: Vec<FileReport>,
    /// Project-discovery warnings in discovery order.
    pub warnings: Vec<String>,
    /// Configured subprocess failures in step and file order.
    pub post_process_failures: Vec<PostProcessFailure>,
}

/// One failed configured subprocess, preserving its file and error detail.
#[derive(Debug)]
pub struct PostProcessFailure {
    /// File supplied to the command.
    pub path: PathBuf,
    /// Configured executable name.
    pub command: String,
    /// Whether spawning failed rather than the child exiting unsuccessfully.
    pub spawn_failed: bool,
    /// Spawn error or child stderr.
    pub message: String,
}

impl RunReport {
    /// Count error-severity findings without treating warnings or hints as
    /// failures.
    ///
    /// # Returns
    ///
    /// The number of `Severity::Error` diagnostics across all files;
    /// `0` when there are none.
    pub fn error_count(&self) -> usize {
        self.files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count()
    }

    /// Check processing success while retaining the complete report for
    /// inspection.
    ///
    /// # Errors
    ///
    /// Returns an `anyhow::Error` when:
    ///
    /// - at least one configured post-processing subprocess failed,
    /// - at least one file could not complete its enabled phases,
    /// - at least one error-severity diagnostic was emitted.
    pub fn ensure_success(&self) -> anyhow::Result<()> {
        if !self.post_process_failures.is_empty() {
            bail!(
                "post_process failed on {} file(s)",
                self.post_process_failures.len()
            );
        }

        let failed = self
            .files
            .iter()
            .filter(|file| file.failure.is_some())
            .count();
        if failed > 0 {
            bail!("failed to process {failed} file(s)");
        }

        let errors = self.error_count();
        if errors > 0 {
            bail!("found {errors} error(s)");
        }
        Ok(())
    }
}
