//! Path resolution utilities: expanding directories, collecting files by
//! extension, and resolving the effective input list (explicit paths or git
//! diff).

use super::Cli;
use crate::diff;
use anyhow::{Context, bail};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Recursively collect all files under `dir` whose extension matches `exts`
/// (ASCII case-insensitively).
///
/// Walks with the `ignore` crate so each repo's own `.gitignore` rules decide
/// what counts as a source input - build output (`target/`), vendored code,
/// and any user-ignored paths are skipped without a hardcoded list.
///
/// Ancestor `.gitignore` files (up to the repo root) apply too, so a
/// subdirectory walk still honours the repo's root rules. Works when the repo
/// root's `.git` is a file (worktree/submodule) as well as a directory.
pub(crate) fn collect_files(
    dir: &Path,
    exts: &[&str],
    out: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    // `hidden(false)` keeps dot-dirs walkable (gitignore still applies), so
    // behaviour matches a plain recursive read.
    //
    // Global/exclude gitignore files are not consulted; only repo
    // `.gitignore` files apply, for reproducible runs independent of the
    // host's global config.
    let walker = WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .parents(true)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type().is_some_and(|ft| ft.is_file())
            && ext_in(path.extension().and_then(|e| e.to_str()), exts)
        {
            out.push(path.to_path_buf());
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
/// admitted exactly like their lowercase forms.
///
/// Non-allocating: compares each candidate byte-wise instead of materializing a
/// lowercase copy.
#[inline]
pub(crate) fn ext_in(ext: Option<&str>, exts: &[&str]) -> bool {
    ext.is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Resolve a list of input paths into a flat, ordered list of files with
/// matching extensions.
pub(crate) fn resolve_all(inputs: &[PathBuf], exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for input in inputs {
        let resolved = resolve_paths(input, exts)
            .with_context(|| format!("failed to resolve path {}", input.display()))?;
        paths.extend(resolved);
    }
    paths.sort();
    paths.dedup();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Deletes the temp dir on drop so a panicked test cannot leak it.
    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Build a throwaway repo (`.git` marker + `.gitignore`) and return the
    /// collected `.rs` files.
    ///
    /// `git_is_file` selects the worktree case where `.git` is a file rather
    /// than a directory. `tag` keeps the temp dir unique per test so
    /// parallel tests cannot clobber each other.
    fn scan_repo(git_is_file: bool, tag: &str) -> Vec<PathBuf> {
        let dir =
            std::env::temp_dir().join(format!("rlt-path-ignore-{}-{}", tag, std::process::id()));
        let guard = TempDir(dir.clone());
        let _guard = guard;
        fs::create_dir_all(&dir).unwrap();
        let git = dir.join(".git");
        if git_is_file {
            fs::write(&git, "gitdir: /nowhere\n").unwrap();
        } else {
            fs::create_dir_all(&git).unwrap();
        }
        fs::write(dir.join(".gitignore"), "target/\nignored.rs\n!kept.rs\n").unwrap();
        fs::write(dir.join("top.rs"), "fn a() {}\n").unwrap();
        fs::write(dir.join("ignored.rs"), "fn b() {}\n").unwrap();
        fs::write(dir.join("kept.rs"), "fn c() {}\n").unwrap();
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target").join("gen.rs"), "fn d() {}\n").unwrap();

        let mut files = Vec::new();
        collect_files(&dir, &["rs"], &mut files).unwrap();
        files.sort();
        files
    }

    /// `.gitignore` is honoured during collection: ignored files and `target/`
    /// (build output) are skipped, while negated entries still come through.
    #[test]
    fn collect_files_follows_gitignore() {
        let files = scan_repo(false, "dir");
        assert_eq!(
            files,
            vec![
                std::env::temp_dir()
                    .join(format!("rlt-path-ignore-dir-{}", std::process::id()))
                    .join("kept.rs"),
                std::env::temp_dir()
                    .join(format!("rlt-path-ignore-dir-{}", std::process::id()))
                    .join("top.rs"),
            ],
            "gitignore rules (ignore + negation) must shape collection"
        );
    }

    /// The worktree/submodule case (.git is a file) must apply gitignore too.
    #[test]
    fn collect_files_follows_gitignore_worktree_git_file() {
        let files = scan_repo(true, "file");
        assert_eq!(
            files.len(),
            2,
            "same ignore rules must apply for a .git file"
        );
    }
}
