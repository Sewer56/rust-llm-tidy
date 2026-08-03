// Vendored from rust-reorder (MIT).
// Atomic file write via tempfile + rename.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Atomically write `content` to `path`.
///
/// Writes to a tempfile in the same directory, then renames onto the
/// target path.  Preserves file permissions by copying them from the
/// existing file (if any).
///
/// # Arguments
///
/// - `path`: the destination file to atomically replace.
/// - `content`: the bytes to write to `path`.
///
/// # Errors
///
/// Returns an error when the temp file cannot be created, written, or
/// renamed onto the target path:
/// - `tempfile::NamedTempFile::new_in` fails to create the temp file (e.g.
///   the directory does not exist or is not writable).
/// - `std::fs::write` fails to write `content` into the temp file.
/// - `tempfile::NamedTempFile::persist` fails to rename the temp file onto
///   the target path.
pub fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));

    // Create a temp file in the same directory for atomic rename.
    let tmp = tempfile::NamedTempFile::new_in(dir).context("failed to create temp file")?;

    fs::write(tmp.path(), content)
        .with_context(|| format!("failed to write temp file {:?}", tmp.path()))?;

    // Preserve permissions from the original file if it exists.
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::set_permissions(tmp.path(), meta.permissions());
    }

    // Atomic rename.
    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("failed to rename temp file onto {:?}: {}", path, e))?;

    Ok(())
}
