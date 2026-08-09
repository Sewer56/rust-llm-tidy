//! Path resolution utilities: expanding directories, collecting files by
//! extension, and resolving the effective input list (explicit paths or git
//! diff).

use super::Cli;
use crate::diff;
use anyhow::{Context, bail};
use std::fs;
use std::path::{Path, PathBuf};

/// Recursively collect all files under `dir` whose extension matches `exts`
/// (ASCII case-insensitively).
pub(crate) fn collect_files(
    dir: &Path,
    exts: &[&str],
    out: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            collect_files(&path, exts, out)?;
        } else if metadata.is_file() && ext_in(path.extension().and_then(|e| e.to_str()), exts) {
            out.push(path);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared path resolution
// ---------------------------------------------------------------------------

/// Resolve the effective input file list: explicit paths (with directory
/// expansion) when given, else changed files from the git diff.
pub(crate) fn resolve_inputs(cli: &Cli, exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    if cli.paths.is_empty() {
        diff::changed_files(exts)
    } else {
        resolve_all(&cli.paths, exts)
    }
}

/// ASCII case-insensitive extension membership check.
///
/// Returns `true` when `ext` (a path extension without the leading dot) matches
/// any entry in `exts` ignoring ASCII case, so `.RS`/`.MD` variants are
/// admitted exactly like their lowercase forms. Non-allocating: compares each
/// candidate byte-wise instead of materializing a lowercase copy.
#[inline]
pub(crate) fn ext_in(ext: Option<&str>, exts: &[&str]) -> bool {
    ext.is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Resolve a list of input paths into a flat, ordered list of files with matching extensions.
pub(crate) fn resolve_all(inputs: &[PathBuf], exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for input in inputs {
        let resolved = resolve_paths(input, exts)
            .with_context(|| format!("failed to resolve path {}", input.display()))?;
        paths.extend(resolved);
    }
    Ok(paths)
}

/// Resolve `path` into a sorted list of files with matching extensions.
///
/// If `path` is a file, it is returned directly. If it is a directory,
/// all files with extensions in `exts` are collected recursively and sorted
/// for deterministic ordering.
fn resolve_paths(path: &Path, exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    if path.is_file() {
        if ext_in(path.extension().and_then(|e| e.to_str()), exts) {
            return Ok(vec![path.to_path_buf()]);
        }
        return Ok(Vec::new());
    }

    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }

    if !path.is_dir() {
        bail!("path is neither a file nor a directory: {}", path.display());
    }

    let mut files = Vec::new();
    collect_files(path, exts, &mut files)
        .with_context(|| format!("failed to read directory {}", path.display()))?;
    files.sort();

    Ok(files)
}
