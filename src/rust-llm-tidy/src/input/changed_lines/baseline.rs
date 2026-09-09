//! Resolve local baselines and compare immutable source snapshots.
//!
//! Batch tree, rename, and blob lookups per repository, then use bounded
//! temporary-file Git diffs to extract added-side line ranges. Explicit
//! references resolve through their merge-base with HEAD.

use super::git_command::{checked, command, output, path};
use super::{ChangedLines, MAX_COLLECTION_BYTES, MAX_SOURCE_BYTES};
use anyhow::{Context, Result, bail};
use core::str::from_utf8;
use std::collections::HashMap;
use std::fs;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Compare immutable bytes, counting only added-side nonempty hunk ranges.
pub(super) fn changed(root: &Path, baseline: &[u8], source: &str) -> Result<ChangedLines> {
    if baseline == source.as_bytes() {
        return Ok(ChangedLines::empty());
    }

    let directory = tempfile::tempdir().context("cannot create snapshot diff directory")?;
    let before = directory.path().join("before");
    let after = directory.path().join("after");
    fs::write(&before, baseline)?;
    fs::write(&after, source)?;
    let (status, patch) = output(
        command(root)
            .args([
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--text",
                "--no-color",
                "--no-renames",
                "--diff-algorithm=myers",
                "--no-indent-heuristic",
                "--unified=0",
                "--inter-hunk-context=0",
                "--output-indicator-new=+",
                "--output-indicator-old=-",
                "--output-indicator-context= ",
                "--exit-code",
                "--",
            ])
            .arg(&before)
            .arg(&after),
    )?;
    if !status.success() && status.code() != Some(1) {
        bail!("Git snapshot diff failed with {status}");
    }

    let mut ranges = Vec::new();
    for line in patch
        .split(|&byte| byte == b'\n')
        .filter(|line| line.starts_with(b"@@ "))
    {
        let header = from_utf8(line)?;
        let added = header
            .split_ascii_whitespace()
            .nth(2)
            .and_then(|value| value.strip_prefix('+'))
            .context("malformed Git hunk range")?;
        let (start, count) = added.split_once(',').unwrap_or((added, "1"));
        let start = start.parse::<usize>()?;
        let count = count.parse::<usize>()?;
        if count > 0 {
            ranges.push(
                start
                    ..=start
                        .checked_add(count - 1)
                        .context("Git hunk range overflow")?,
            );
        }
    }

    Ok(ChangedLines::new(ranges))
}

/// Resolve HEAD or the merge-base of an explicit commit reference and HEAD.
pub(super) fn resolve(root: &Path, baseline: Option<&str>) -> Result<Option<String>> {
    let (head_status, head) =
        output(command(root).args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]))?;
    if !head_status.success() {
        if baseline.is_some() {
            bail!(
                "explicit baseline requires an existing HEAD in {}",
                root.display()
            );
        }
        return Ok(None);
    }
    let head = from_utf8(&head)?.trim();
    let Some(reference) = baseline else {
        return Ok(Some(head.to_owned()));
    };

    let revision = format!("{reference}^{{commit}}");
    let resolved =
        checked(command(root).args(["rev-parse", "--verify", "--end-of-options", &revision]))
            .with_context(|| {
                format!(
                    "cannot resolve explicit baseline {reference:?} in {}",
                    root.display()
                )
            })?;
    let resolved = from_utf8(&resolved)?.trim();
    let base = checked(command(root).args(["merge-base", resolved, head]))
        .with_context(|| format!("baseline {reference:?} has no available merge-base with HEAD"))?;

    Ok(Some(from_utf8(&base)?.trim().to_owned()))
}

/// Read only requested blobs in one batch, retaining staged rename provenance.
///
/// The tree and cached rename inventory are collected once per repository.
/// Unstaged moves to untracked paths are new files, as Git does not track them.
pub(super) fn sources(
    root: &Path,
    baseline: &str,
    paths: &[PathBuf],
) -> Result<HashMap<PathBuf, Vec<u8>>> {
    let tree = checked(command(root).args(["ls-tree", "-r", "-z", "-l", baseline]))?;
    let mut entries = HashMap::new();
    for record in tree
        .split(|&byte| byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|&byte| byte == b'\t')
            .context("malformed Git tree record")?;
        let mut fields = from_utf8(&record[..tab])?.split_ascii_whitespace();
        let mode = fields.next().context("missing Git tree mode")?;
        let kind = fields.next().context("missing Git tree type")?;
        let oid = fields.next().context("missing Git tree object")?;
        let size = fields.next().context("missing Git tree size")?;
        if kind == "blob" && matches!(mode, "100644" | "100755") {
            entries.insert(path(&record[tab + 1..])?, (oid, size.parse::<usize>()?));
        }
    }

    let renames = staged_renames(root, baseline)?;
    let mut requests = tempfile::tempfile().context("cannot create Git batch request file")?;
    let mut selected = Vec::with_capacity(paths.len());
    let mut total = 0usize;
    for input in paths {
        let original = renames.get(input).unwrap_or(input);
        if let Some(&(oid, size)) = entries.get(original) {
            if size > MAX_SOURCE_BYTES {
                bail!(
                    "baseline {} exceeds {MAX_SOURCE_BYTES} bytes",
                    input.display()
                );
            }
            total = total
                .checked_add(size + oid.len() + 64)
                .context("baseline size overflow")?;
            if total > MAX_COLLECTION_BYTES {
                bail!("baseline batch exceeds {MAX_COLLECTION_BYTES} bytes; narrow the input list");
            }
            writeln!(requests, "{oid}")?;
            selected.push((input, size));
        }
    }
    if selected.is_empty() {
        return Ok(HashMap::new());
    }

    requests.rewind()?;
    let bytes = checked(
        command(root)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::from(requests)),
    )?;
    let mut remaining = bytes.as_slice();
    let mut sources = HashMap::with_capacity(selected.len());
    for (input, size) in selected {
        let end = remaining
            .iter()
            .position(|&byte| byte == b'\n')
            .context("missing Git blob header")?;
        let header = from_utf8(&remaining[..end])?;
        let mut fields = header.split_ascii_whitespace();
        fields.next();
        if fields.next() != Some("blob")
            || fields.next().and_then(|value| value.parse().ok()) != Some(size)
        {
            bail!(
                "Git batch returned an unavailable or unexpected blob for {}",
                input.display()
            );
        }
        remaining = &remaining[end + 1..];
        let source = remaining.get(..size).context("truncated Git blob")?;
        sources.insert(input.clone(), source.to_vec());
        if remaining.get(size) != Some(&b'\n') {
            bail!("missing Git blob terminator");
        }
        remaining = &remaining[size + 1..];
    }

    Ok(sources)
}

/// Map index destinations to baseline names without working-tree filters.
fn staged_renames(root: &Path, baseline: &str) -> Result<HashMap<PathBuf, PathBuf>> {
    // Include committed moves since a merge-base as well as staged renames.
    let raw = checked(command(root).args([
        "diff",
        "--cached",
        "--raw",
        "-z",
        "--no-abbrev",
        "--no-ext-diff",
        "--no-textconv",
        "--no-relative",
        "--find-renames",
        "-l1000",
        baseline,
        "--",
    ]))?;
    let mut records = raw
        .split(|&byte| byte == 0)
        .filter(|record| !record.is_empty());

    let mut renames = HashMap::new();
    while let Some(header) = records.next() {
        let status = from_utf8(header)?
            .split_ascii_whitespace()
            .last()
            .context("missing Git raw status")?;
        let old = records.next().context("missing Git raw path")?;
        if status.starts_with(['R', 'C']) {
            let new = records.next().context("missing Git rename destination")?;
            renames.insert(path(new)?, path(old)?);
        }
    }

    Ok(renames)
}
