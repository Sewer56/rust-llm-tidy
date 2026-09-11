//! Git-diff file collection for `rust-llm-tidy`.
//!
//! With no path arguments, [`changed_files`] collects tracked files changed in
//! the current `git` diff (staged and unstaged) plus untracked, non-ignored
//! files.
//!
//! It keeps only paths matching the caller's extensions, skipping deletions
//! and missing files.
//!
//! Shells out to `git` via `std::process::Command`; no new dependencies.

use anyhow::{Context, anyhow, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Changed files in the current git repository, filtered to `exts`.
///
/// Combines unstaged and staged tracked paths with untracked, non-ignored
/// files, excluding deletions.
///
/// Returns absolute `PathBuf`s rooted at the repo top-level, sorted and
/// deduped, that exist and match `exts`. Works from any cwd inside the repo.
///
/// # Arguments
///
/// Select which file extensions to keep.
///
/// - `exts`: file extensions without the leading dot, such as `"rs"`
/// - `exclude_license_documents`: skip conventional license documents when true
/// - A changed path is returned only when its extension matches an entry in `exts`.
///
/// # Errors
///
/// Returns an error if a `git` invocation fails.
///
/// Failure cases:
///
/// - `git rev-parse --show-toplevel` cannot determine the repo root (e.g. the
///   current directory is outside a git repository)
/// - a `git diff` or `git ls-files` invocation fails
///
/// This is an `anyhow::Result`, so any upstream I/O or `git` failure is
/// propagated as the error.
pub fn changed_files(
    exts: &[&str],
    exclude_license_documents: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    let root_raw = git_stdout(&["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root_raw.trim());
    let mut paths = Vec::new();

    for line in changed_lines(&root)? {
        let p = root.join(line);
        if matches_ext(&p, exts)
            && !(exclude_license_documents && super::is_license_document(&p))
            && p.is_file()
        {
            paths.push(p);
        }
    }

    paths.sort();
    paths.dedup();

    Ok(paths)
}

fn changed_lines(root: &Path) -> anyhow::Result<Vec<String>> {
    // Run at the repo root so discovery is repo-wide, not scoped to the
    // invocation directory. `git ls-files --others` is cwd-scoped, while
    // `git diff` is not.
    //
    // `-z` separates paths with NUL bytes and avoids Git's quoting or escaping
    // of path names; the lossy UTF-8 conversion in `git_stdout_opt` still
    // replaces non-UTF-8 bytes.
    let mut paths = nul_paths(
        &git_stdout_opt(
            Some(root),
            &[
                "diff",
                "--no-relative",
                "--name-only",
                "--diff-filter=ACMR",
                "-z",
            ],
        )?
        .unwrap_or_default(),
    );
    paths.extend(nul_paths(
        &git_stdout_opt(
            Some(root),
            &[
                "diff",
                "--no-relative",
                "--cached",
                "--name-only",
                "--diff-filter=ACMR",
                "-z",
            ],
        )?
        .unwrap_or_default(),
    ));
    // Untracked, non-ignored files count as new. At the repo root, paths are
    // repo-root-relative, matching `git diff --no-relative`.
    paths.extend(nul_paths(
        &git_stdout_opt(
            Some(root),
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "--full-name",
                "-z",
            ],
        )?
        .unwrap_or_default(),
    ));
    Ok(paths)
}

fn git_stdout(args: &[&str]) -> anyhow::Result<String> {
    git_stdout_opt(None, args)?
        .ok_or_else(|| anyhow!("`git {}` produced no output", args.join(" ")))
}

fn matches_ext(p: &Path, exts: &[&str]) -> bool {
    super::ext_in(p.extension().and_then(|e| e.to_str()), exts)
}

fn git_stdout_opt(cwd: Option<&Path>, args: &[&str]) -> anyhow::Result<Option<String>> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let out = command
        .args(args)
        .output()
        .with_context(|| "failed to run `git` (is it installed and on PATH?)")?;
    if !out.status.success() {
        bail!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
}

fn nul_paths(output: &str) -> Vec<String> {
    output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(String::from)
        .collect()
}
