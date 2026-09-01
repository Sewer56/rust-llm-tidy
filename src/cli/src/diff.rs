//! Git-diff file collection for `rust-llm-tidy`.
//!
//! When the CLI is invoked with no path arguments, [`changed_files`]
//! collects tracked files changed in the current `git` diff (staged +
//! unstaged), filtered to the caller's extensions and skipping deletions
//! and missing files.
//!
//! Shells out to `git` via `std::process::Command`; no new dependencies.

use anyhow::{Context, anyhow, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Changed files in the current git repository, filtered to `exts`.
///
/// Combines unstaged and staged tracked paths (deletions excluded), deduped
/// and rooted at the repo top-level so it works from any cwd inside the repo.
/// Returns absolute, sorted, deduped `PathBuf`s that exist and match `exts`.
///
/// # Arguments
///
/// - `exts`: file extensions (without the leading dot, e.g. `"rs"`) to keep.
///   A changed path is returned only when its extension matches one of these.
///
/// # Errors
///
/// Returns an error if `git rev-parse --show-toplevel` cannot determine the
/// repo root (e.g. the current directory is outside a git repository) or if a
/// `git diff` invocation fails.
///
/// This is an `anyhow::Result`, so any upstream I/O or `git` failure is
/// propagated as the error.
pub fn changed_files(exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    let root_raw = git_stdout(&["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root_raw.trim());
    let mut paths = Vec::new();
    for line in changed_lines(&root)? {
        let p = root.join(line);
        if matches_ext(&p, exts) && p.is_file() {
            paths.push(p);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn changed_lines(_root: &Path) -> anyhow::Result<Vec<String>> {
    // Combine tracked paths. NUL output keeps Git's path names
    // verbatim, including names requiring quoting.
    let mut paths = nul_paths(
        &git_stdout_opt(&[
            "diff",
            "--no-relative",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ])?
        .unwrap_or_default(),
    );
    paths.extend(nul_paths(
        &git_stdout_opt(&[
            "diff",
            "--no-relative",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ])?
        .unwrap_or_default(),
    ));
    Ok(paths)
}

fn git_stdout(args: &[&str]) -> anyhow::Result<String> {
    git_stdout_opt(args)?.ok_or_else(|| anyhow!("`git {}` produced no output", args.join(" ")))
}

fn matches_ext(p: &Path, exts: &[&str]) -> bool {
    crate::paths::ext_in(p.extension().and_then(|e| e.to_str()), exts)
}

fn git_stdout_opt(args: &[&str]) -> anyhow::Result<Option<String>> {
    let out = Command::new("git")
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
