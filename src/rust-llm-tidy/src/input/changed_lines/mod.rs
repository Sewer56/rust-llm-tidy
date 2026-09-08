//! Capture Git changed lines and remap their eligibility after transforms.
//!
//! Call [`collect`] only after granting Git and input-file read permissions.
//! Collection does not select lints, change files, or fetch remote objects.
//! Paths in its result retain the caller's spelling.

pub use collection::{ChangedLineCollection, collect};
pub use ranges::ChangedLines;
pub use snapshot::ChangedLineSnapshot;
use std::path::Path;

mod baseline;
mod collection;
mod git_command;
mod ranges;
mod snapshot;
#[cfg(test)]
mod tests;

/// Maximum retained source bytes or bytes emitted by one Git subprocess.
pub const MAX_COLLECTION_BYTES: usize = 64 * 1024 * 1024;
/// Maximum input paths accepted by one collection call, before deduplication.
pub const MAX_INPUT_PATHS: usize = 4096;
/// Maximum bytes in one source or baseline blob, and in remapped output.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum lines in one snapshot or remapped output.
pub const MAX_SOURCE_LINES: usize = 256 * 1024;

/// Validate an explicit local baseline even when no lint needs snapshots.
pub(crate) fn validate_baseline(directory: &Path, reference: &str) -> anyhow::Result<()> {
    let directory = if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    };
    baseline::resolve(directory, Some(reference))?;
    Ok(())
}
