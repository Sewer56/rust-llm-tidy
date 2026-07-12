//! Git-diff file collection for `rust-llm-tidy`.
//!
//! When the CLI is invoked with no path arguments, [`changed_files`]
//! collects the files changed in the current `git` diff (staged + unstaged
//! vs `HEAD`, plus untracked), filtered to the caller's extensions and
//! skipping deletions and missing files. Shells out to `git` via
//! `std::process::Command`; no new dependencies.

use anyhow::{Context, anyhow, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Changed files in the current git repository, filtered to `exts`.
///
/// Combines `git diff HEAD` (staged + unstaged, deletions excluded) with
/// untracked files (`git ls-files --others --exclude-standard`), deduped and
/// rooted at the repo top-level so it works from any cwd inside the repo.
/// Returns absolute, sorted, deduped `PathBuf`s that exist and match `exts`.
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
    // Try `git diff HEAD` (normal case). On an unborn HEAD (fresh repo, no
    // commits) fall back to unstaged + staged diffs against the empty tree.
    match git_stdout_opt(&["diff", "--name-only", "--diff-filter=ACMR", "HEAD"]) {
        Ok(Some(s)) if !s.trim().is_empty() => {
            return Ok(s.lines().map(String::from).collect());
        }
        Ok(_) => {}
        Err(_) => {}
    }
    let mut out = String::new();
    out.push_str(&git_stdout(&["diff", "--name-only", "--diff-filter=ACMR"])?);
    out.push('\n');
    out.push_str(&git_stdout(&[
        "diff",
        "--cached",
        "--name-only",
        "--diff-filter=ACMR",
    ])?);
    out.push('\n');
    out.push_str(&git_stdout(&[
        "ls-files",
        "--others",
        "--exclude-standard",
    ])?);
    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

fn git_stdout(args: &[&str]) -> anyhow::Result<String> {
    git_stdout_opt(args)?.ok_or_else(|| anyhow!("`git {}` produced no output", args.join(" ")))
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

fn matches_ext(p: &Path, exts: &[&str]) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.contains(&e))
}
