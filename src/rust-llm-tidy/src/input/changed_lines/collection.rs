//! Explicit input-file snapshot collection grouped by repository.

use super::git_command::{command, output, path};
use super::{
    ChangedLineSnapshot, ChangedLines, MAX_COLLECTION_BYTES, MAX_INPUT_PATHS, MAX_SOURCE_BYTES,
    MAX_SOURCE_LINES, baseline,
};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Per-input snapshots and warnings for unavailable implicit baselines.
#[derive(Debug, Default)]
pub struct ChangedLineCollection {
    /// Original caller paths mapped to snapshots, or `None` without a baseline.
    pub snapshots: BTreeMap<PathBuf, Option<ChangedLineSnapshot>>,
    /// One warning per unavailable repository or standalone input directory.
    pub warnings: Vec<String>,
}

/// Capture net baseline-to-current-source line eligibility before mutations.
///
/// With no `baseline`, compare HEAD to the input working-tree bytes, not the
/// union of staged and unstaged hunks. An explicit reference uses its merge-base
/// with HEAD in each repository. No remote fetches are allowed.
///
/// Repository discovery starts at each input's parent, independent of cwd, and
/// is cached by directory. Tree, staged rename, and blob lookups are batched per
/// repository.
///
/// Each differing tracked file uses one immutable temporary-file diff.
/// New and untracked files include all current lines. Deleted inputs have
/// an empty source and no eligible lines. Staged renames retain baseline content;
/// an unstaged move to an untracked name is treated as a new file.
///
/// Callers must grant Git and input-file reads before invoking this function and
/// prevent concurrent input/index changes during collection. The function does
/// not change the index or working tree.
///
/// Without an implicit baseline, snapshots
/// are `None` with a warning; callers choose the fallback policy.
///
/// # Arguments
/// - `paths`: explicit file paths, not directories, relative to cwd or absolute.
/// - `baseline`: local commit reference, or `None` to use each repository's HEAD.
///
/// # Remarks
/// Limits are [`MAX_INPUT_PATHS`], [`MAX_SOURCE_BYTES`], [`MAX_SOURCE_LINES`], and
/// [`MAX_COLLECTION_BYTES`]. Rename similarity search is capped at 1,000 paths;
/// Git may classify larger or dissimilar moves as new files. Runtime has not been
/// measured. Git subprocesses have bounded captured output, not a wall-clock
/// timeout. A non-UTF-8 Git path is preserved on Unix and rejected elsewhere.
///
/// # Errors
/// Returns [`anyhow::Error`] for these conditions:
///
/// - An explicit reference cannot resolve, HEAD is absent, its merge-base is
///   unavailable, or an input is outside Git: choose a locally available baseline.
/// - A source is unreadable, non-UTF-8, nonregular, or over a collection limit:
///   provide readable text files or narrow the input list.
/// - An input parent cannot be read: check directory permissions.
/// - Git cannot start, read output, complete successfully, or return the expected
///   protocol: check local Git installation and repository integrity.
/// - Temporary snapshot or batch files cannot be created, written, or rewound:
///   provide a writable temporary directory with enough space.
pub fn collect(paths: &[PathBuf], baseline: Option<&str>) -> Result<ChangedLineCollection> {
    if paths.len() > MAX_INPUT_PATHS {
        bail!("input list exceeds {MAX_INPUT_PATHS} paths; narrow the input list");
    }

    let mut collection = ChangedLineCollection::default();
    let mut directories: BTreeMap<PathBuf, Option<PathBuf>> = BTreeMap::new();
    let mut repositories: BTreeMap<PathBuf, Vec<(PathBuf, PathBuf)>> = BTreeMap::new();
    for input in paths {
        if collection.snapshots.contains_key(input) {
            continue;
        }
        let parent = input
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent
            .canonicalize()
            .with_context(|| format!("cannot discover input directory {}", parent.display()))?;
        let absolute = parent.join(input.file_name().context("input path has no filename")?);
        if !directories.contains_key(&parent) {
            let (status, root) = output(command(&parent).args(["rev-parse", "--show-toplevel"]))?;
            let root = if status.success() {
                let root = root.strip_suffix(b"\n").unwrap_or(&root);
                Some(path(root)?.canonicalize()?)
            } else {
                if baseline.is_some() {
                    bail!(
                        "explicit baseline requires a Git repository for {}",
                        input.display()
                    );
                }
                collection.warnings.push(format!(
                    "changed-line baseline unavailable in {}; no Git working tree",
                    parent.display()
                ));
                None
            };
            directories.insert(parent.clone(), root);
        }

        collection.snapshots.insert(input.clone(), None);
        if let Some(root) = &directories[&parent] {
            let relative = absolute
                .strip_prefix(root)
                .context("input is outside discovered Git root")?
                .to_path_buf();
            repositories
                .entry(root.clone())
                .or_default()
                .push((input.clone(), relative));
        }
    }

    let mut total = 0usize;
    for (root, inputs) in repositories {
        let Some(base) = baseline::resolve(&root, baseline)? else {
            collection.warnings.push(format!(
                "changed-line baseline unavailable in {}; HEAD has no local commit",
                root.display()
            ));
            continue;
        };
        let relative: Vec<_> = inputs
            .iter()
            .map(|(_, relative)| relative.clone())
            .collect();
        let sources = baseline::sources(&root, &base, &relative)
            .with_context(|| format!("cannot read baseline in {}", root.display()))?;

        for (input, relative) in inputs {
            let source = read_source(&root.join(&relative))?;
            total = total
                .checked_add(source.len())
                .context("snapshot size overflow")?;
            if total > MAX_COLLECTION_BYTES {
                bail!(
                    "snapshot sources exceed {MAX_COLLECTION_BYTES} bytes; narrow the input list"
                );
            }

            let changed = match sources.get(&relative) {
                Some(before) => baseline::changed(&root, before, &source)?,
                None => ChangedLines::all(&source),
            };
            collection.snapshots.insert(
                input,
                Some(ChangedLineSnapshot {
                    source: source.into_boxed_str(),
                    changed,
                }),
            );
        }
    }

    Ok(collection)
}

/// Read bounded regular UTF-8 input; a deleted file has an empty current source.
fn read_source(path: &Path) -> Result<String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot inspect {}", path.display()));
        }
    };
    if !metadata.is_file() {
        bail!("snapshot input must be a regular file: {}", path.display());
    }
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        bail!("source {} exceeds {MAX_SOURCE_BYTES} bytes", path.display());
    }

    let mut source = String::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .with_context(|| format!("cannot read UTF-8 source {}", path.display()))?;
    if source.len() > MAX_SOURCE_BYTES || source.lines().count() > MAX_SOURCE_LINES {
        bail!(
            "source {} exceeds snapshot byte or line limits",
            path.display()
        );
    }

    Ok(source)
}
