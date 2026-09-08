//! Coordinate file transformations, project facts and linting without terminal
//! output.
//!
//! # Module map
//!
//! - `run`/`should_parallelize`/`validate_selection`/`dedup_inputs` (this
//!   file): orchestration, input collapsing, and selection validation
//! - `buffer`: standalone source-buffer processing shared by entry points
//! - `comment_fixes`: comment-run text fixes shared by buffer and file paths
//! - `file_execution`: per-file mutation and lint phase execution
//! - `files`: file I/O operations and the crate-aware visibility context
//! - `run_options`: explicit permissions and rule selection for `run`
//! - `source_options`: options for standalone buffer processing

use crate::config::{self, CompiledConfig, FilePolicy, PostProcessStep};
use crate::input as paths;
use crate::languages::registry as langs;
use crate::project::csharp::CSharpIndex;
use crate::reporting::{FileReport, PostProcessFailure, RunReport};
use crate::rules::registry as check;
pub use buffer::tidy_source;
use rayon::prelude::*;
pub use run_options::RunOptions;
pub use source_options::SourceOptions;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod buffer;
mod comment_fixes;
mod file_execution;
mod files;
mod lint_context;
#[cfg(test)]
mod lint_context_tests;
mod run_options;
mod source_options;

impl FileReport {
    /// Stop this file after an execution failure, retaining earlier changes.
    fn fail(&mut self, error: &anyhow::Error) {
        self.failure = Some(format!("{error:?}"));
        self.processed = false;
    }
}

/// Process selected files using explicit write and subprocess permissions.
///
/// Returns partial results when individual files or configured subprocesses
/// fail.
/// Inspect [`RunReport::ensure_success`] after consuming the report.
/// Configuration discovery is explicit through [`crate::config`].
///
/// The opt-in TEXT007 lint runs only when the config's
/// `passive_narration.enable` setting is on or the selection names the
/// code explicitly.
///
/// File previews leave each pass reading the original disk source; see
/// [`tidy_source`] for final-buffer linting.
/// Cargo project discovery requires `options.cargo_discovery`; otherwise
/// Rust visibility uses standalone facts.
///
/// # Arguments
///
/// - `options`: inputs, selections, and explicit execution permissions
/// - `config`: previously loaded configuration, or language defaults when
///   absent
///
/// # Example
///
/// ```rust
/// use rust_llm_tidy::{RunOptions, run};
///
/// let directory = tempfile::tempdir()?;
/// let path = directory.path().join("input.rs");
/// std::fs::write(&path, "pub fn load() {}\n")?;
/// let options = RunOptions {
///     paths: vec![path],
///     include: vec!["DOC001".into()],
///     ..RunOptions::default()
/// };
/// let report = run(&options, None)?;
/// assert_eq!(report.error_count(), 1);
/// assert_eq!(report.files[0].diagnostics[0].code, "DOC001");
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Failures use [`anyhow::Error`] with the failing operation's context.
///
/// - Unknown rule selection: an included or excluded name is not registered
/// - Malformed extension: an extra extension cannot match a file suffix
/// - Input discovery failure: a selected path is missing, unreadable, or Git
///   lookup fails
/// - Project discovery failure: a C# project source directory cannot be
///   traversed
///
/// Changed-line reporting failures:
///
/// - Explicit baseline failure: the reference or HEAD cannot resolve locally,
///   or their merge-base is unavailable, even when no inputs are selected
/// - Snapshot failure: a scoped input or baseline cannot be read as bounded
///   UTF-8 source, or Git output exceeds the collection limits in
///   [`crate::input::changed_lines`]
///
/// # Remarks
///
/// License documents are excluded by default for all path selections. Set
/// `exclude_license_documents: false` in config to disable this filter;
/// see [`crate::input`] for filename matching.
pub fn run(options: &RunOptions, config: Option<&CompiledConfig>) -> anyhow::Result<RunReport> {
    validate_selection(&options.include, &options.exclude, &options.extensions)?;

    let allowed = langs::allowed_extensions(config, &options.extensions);
    let discovery = [PathBuf::from(".")];
    let inputs = if options.paths.is_empty() && options.diff_base.is_some() {
        discovery.as_slice()
    } else {
        &options.paths
    };
    let paths = dedup_inputs(paths::resolve_inputs(
        inputs,
        options.git_changed,
        &allowed,
        config.is_none_or(CompiledConfig::exclude_license_documents),
    )?);
    let mut report = RunReport::default();
    let included: Option<HashSet<String>> =
        (!options.include.is_empty()).then(|| options.include.iter().cloned().collect());
    let disabled: HashSet<String> = options.exclude.iter().cloned().collect();
    let mut lint_context = lint_context::LintContext::new(config, options.lint_scope);
    report.warnings = lint_context.capture(&paths, options, included.as_ref(), &disabled)?;
    if paths.is_empty() {
        return Ok(report);
    }
    let visibility_inputs: Vec<_> = if options.cargo_discovery {
        paths
            .iter()
            .filter(|path| {
                let policy = effective_policy(path, config, included.as_ref(), &disabled);
                !policy.skip
                    && paths::ext_in(path.extension().and_then(|ext| ext.to_str()), &["rs"])
                    && langs::profile_for("rs").op_enabled("vis", &policy.enabled, &policy.disabled)
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let context = if !visibility_inputs.is_empty() {
        files::resolve_vis_context(&visibility_inputs, &mut report.warnings)
    } else {
        None
    };

    let parallel = should_parallelize(&paths);
    let lints_may_run = !disabled.contains("lints")
        && included.as_ref().is_none_or(|set| {
            set.contains("lints") || check::LINT_CODES.iter().any(|code| set.contains(*code))
        });
    let mut csharp = lints_may_run
        .then(|| CSharpIndex::build(&paths))
        .transpose()?;

    let mutate = |path: &PathBuf| {
        file_execution::process_one(
            path,
            included.as_ref(),
            &disabled,
            context.as_ref(),
            !options.apply,
            (None, None),
            &lint_context,
        )
    };
    let results: Vec<_> = if parallel {
        paths.par_iter().map(mutate).collect()
    } else {
        paths.iter().map(mutate).collect()
    };

    // Cross-file lint facts must reflect all completed writes, not a partial batch.
    if let Some(index) = &mut csharp {
        index.refresh(&paths);
    }
    let lint = |(path, out): (&PathBuf, FileReport)| {
        if out.failure.is_some() {
            return out;
        }
        file_execution::process_one(
            path,
            included.as_ref(),
            &disabled,
            context.as_ref(),
            !options.apply,
            (Some(out), csharp.as_ref()),
            &lint_context,
        )
    };
    report.files = if parallel {
        paths
            .par_iter()
            .zip(results.into_par_iter())
            .map(lint)
            .collect()
    } else {
        paths.iter().zip(results).map(lint).collect()
    };

    if options.apply
        && options.post_process
        && let Some(config) = config
    {
        let processed = files::post_process_inputs(&mut report, lint_context.rules());
        report.post_process_failures = run_post_process(config.post_process_steps(), &processed);
    }
    Ok(report)
}

/// Whether per-file processing should run on rayon's work-stealing pool.
///
/// Run parallel once there is input work enough to clear the pool-overhead
/// floor. A single input never parallelizes - nothing to split.
///
/// # Scoring
///
/// Each file scores `byte length × per-type weight`, all weights relative to
/// markdown = 1000:
///
/// - `.rs`: 120_000 - reorder/vis/lints run ~0.26 ms/KB, plus a fixed
///   ~2-3ms per-file parse cost; the weight folds both in.
/// - `.md`: 1_000 - the `fix_*` ops are ~0.007 ms/KB scans.
/// - anything cheaper than markdown: pick a weight below 1_000 (e.g. plain
///   text ~100) — it still lands in the one formula.
///
/// Scores sum; past 600K markdown-equivalent bytes with more than one input
/// -> parallelize.
///
/// # Calibration
///
/// Weights = 120x markdown and the 600KB score minimize regret over 26
/// measured workloads. Neither is sensitive.
///
/// Workloads span single-threaded vs 32-thread runs. Each value can float
/// ±50% before regret exceeds 0.5ms.
///
/// Early-exits on the threshold, so huge repos don't `stat` every file.
pub(crate) fn should_parallelize(paths: &[PathBuf]) -> bool {
    // Fixed-point scale so sub-markdown types (weight < 1000) stay integer.
    // Score = Σ (byte size × weight).
    const WEIGHT_SCALE: u64 = 1000;
    // Markdown is the baseline: 1000 == 1 markdown byte.
    const MARKDOWN_WEIGHT: u64 = WEIGHT_SCALE;
    // Rust bytes count 120x markdown (calibrated, see above).
    const RUST_WEIGHT: u64 = 120 * WEIGHT_SCALE;
    // Parallelize once the weighted score clears 600K markdown-equivalent
    // bytes (≈5KB of Rust).
    const PARALLEL_SCORE: u64 = 600 * 1024 * WEIGHT_SCALE;

    /// Byte weight of one file by extension, in [`WEIGHT_SCALE`] units.
    ///
    /// - `1000` is markdown (the baseline).
    /// - Anything cheaper than markdown can be added below it.
    /// - Non-Rust inputs are text-tier scans, so they fall back to the markdown weight.
    fn byte_weight(ext: Option<&str>) -> u64 {
        if crate::input::ext_in(ext, &["rs"]) {
            RUST_WEIGHT
        } else {
            MARKDOWN_WEIGHT
        }
    }

    if paths.len() < 2 {
        return false;
    }
    let mut score = 0u64;
    for p in paths {
        let w = byte_weight(p.extension().and_then(|e| e.to_str()));
        score = score.saturating_add(
            std::fs::metadata(p)
                .map(|m| m.len().saturating_mul(w))
                .unwrap_or(0),
        );
        if score >= PARALLEL_SCORE {
            return true;
        }
    }
    false
}

/// Validate explicit selection before allocating per-file processing state.
pub(crate) fn validate_selection(
    include: &[String],
    exclude: &[String],
    extensions: &[String],
) -> anyhow::Result<()> {
    let valid = config::known_rules();
    for name in include.iter().chain(exclude) {
        if !valid.contains(&name.as_str()) {
            anyhow::bail!(
                "unknown op/rule `{name}` in --include/--exclude; valid: {}",
                valid.join(", ")
            );
        }
    }

    for ext in extensions {
        langs::validate_extension(ext)?;
    }
    Ok(())
}

/// Collapse path aliases before dispatch.
///
/// The input resolver dedups literal paths only, so one inode reachable under
/// two spellings (`.` vs `./src`, a symlink, or a dir-walk plus an explicit
/// file) would otherwise be processed twice.
///
/// In parallel, both copies run on the original source and emit duplicate
/// change records.
///
/// Each inode keeps its first spelling, so displayed paths and output order
/// are unchanged.
///
/// Canonicalization covers relative/absolute differences and symlinks. On
/// Unix a `(dev, ino)` key also catches hardlinks, which
/// canonicalization cannot (distinct paths, one inode).
fn dedup_inputs(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut by_path: HashSet<PathBuf> = HashSet::new();
    #[cfg(unix)]
    let mut by_inode: HashSet<(u64, u64)> = HashSet::new();

    paths
        .into_iter()
        .filter(|p| {
            let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            if !by_path.insert(canon) {
                return false;
            }
            #[cfg(unix)]
            use std::os::unix::fs::MetadataExt;
            #[cfg(unix)]
            {
                match std::fs::metadata(p) {
                    Ok(m) => by_inode.insert((m.dev(), m.ino())),
                    Err(_) => true, // unstat-able; path key already accepted it
                }
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
        .collect()
}

/// Resolve configuration and explicit selections before discovery or execution.
fn effective_policy(
    path: &Path,
    config: Option<&CompiledConfig>,
    included: Option<&HashSet<String>>,
    disabled: &HashSet<String>,
) -> FilePolicy {
    let mut policy = config
        .map(|config| config.policy_for(path))
        .unwrap_or_default();
    if policy.skip {
        return policy;
    }

    if let Some(included) = included {
        policy.enabled = Some(included.clone());
        policy.disabled.clear();
    }
    policy.disabled.extend(disabled.iter().cloned());
    if let Some(enabled) = &mut policy.enabled {
        enabled.retain(|rule| !disabled.contains(rule));
    }
    policy
}

/// Execute configured commands only for eligible files, without invoking a
/// shell.
fn run_post_process(steps: &[PostProcessStep], files: &[PathBuf]) -> Vec<PostProcessFailure> {
    let mut failures = Vec::new();
    for step in steps {
        let exts: Vec<_> = step.extensions.iter().map(String::as_str).collect();
        for file in files {
            if !exts.is_empty()
                && !paths::ext_in(file.extension().and_then(|ext| ext.to_str()), &exts)
            {
                continue;
            }

            let output = std::process::Command::new(&step.command)
                .args(&step.args)
                .arg(file)
                .output();
            let (spawn_failed, message) = match output {
                Ok(out) if out.status.success() => continue,
                Ok(out) => (
                    false,
                    String::from_utf8_lossy(&out.stderr).trim().to_string(),
                ),
                Err(error) => (true, error.to_string()),
            };
            failures.push(PostProcessFailure {
                path: file.clone(),
                command: step.command.clone(),
                spawn_failed,
                message,
            });
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::dedup_inputs;
    use super::file_execution::process_one;
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::fs;
    use std::path::PathBuf;

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    /// A read failure between phases revokes post-processing eligibility.
    /// Successful linting retains eligibility even when diagnostics report
    /// errors.
    #[test]
    fn lint_phase_should_exclude_failed_reads_from_processed_files() {
        let dir = temp_dir();
        let path = dir.join("Caller.cs");
        let disabled = std::collections::HashSet::new();
        let included = ["tables".to_string(), "lints".to_string()]
            .into_iter()
            .collect();

        for (label, source, remove, errors) in [
            ("clean", "class C {}", false, false),
            (
                "diagnostic",
                "class C { public void Caller() { throw new E(); } }",
                false,
                true,
            ),
            ("read_failure", "class C {}", true, false),
        ] {
            fs::write(&path, source).unwrap();
            let mutated = process_one(
                &path,
                Some(&included),
                &disabled,
                None,
                false,
                (None, None),
                &super::lint_context::LintContext::new(None, None),
            );
            assert!(mutated.processed, "{label}");
            if remove {
                fs::remove_file(&path).unwrap();
            }

            let linted = process_one(
                &path,
                Some(&included),
                &disabled,
                None,
                false,
                (Some(mutated), None),
                &super::lint_context::LintContext::new(None, None),
            );

            assert_eq!(linted.failure.is_some(), remove, "{label}");
            assert_eq!(linted.processed, !remove, "{label}");
            assert_eq!(
                linted
                    .diagnostics
                    .iter()
                    .any(|d| d.severity == crate::reporting::Severity::Error),
                errors,
                "{label}"
            );
        }

        cleanup(&dir);
    }

    fn temp_dir() -> PathBuf {
        let n = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("rust-llm-tidy-dedup-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn cleanup(d: &PathBuf) {
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn dedups_aliases_preserving_first_spelling() {
        let dir = temp_dir();
        fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
        fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();
        // Same inode spelled three ways: plain, `./` component, literal
        // duplicate. Only the first spelling must survive, in order.
        let input = vec![
            dir.join("a.rs"),
            dir.join(".").join("a.rs"),
            dir.join("b.rs"),
            dir.join("a.rs"),
        ];
        assert_eq!(
            dedup_inputs(input),
            vec![dir.join("a.rs"), dir.join("b.rs")]
        );
        cleanup(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn dedups_symlink_and_hardlink_aliases() {
        let dir = temp_dir();
        fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
        std::os::unix::fs::symlink(dir.join("a.rs"), dir.join("link.rs")).unwrap();
        // Hardlink: distinct canonical path, same (dev, ino) - a symlink-only
        // dedup would miss it.
        fs::hard_link(dir.join("a.rs"), dir.join("hard.rs")).unwrap();

        let out = dedup_inputs(vec![
            dir.join("a.rs"),
            dir.join("link.rs"),
            dir.join("hard.rs"),
        ]);
        assert_eq!(out, vec![dir.join("a.rs")]);
        cleanup(&dir);
    }

    #[test]
    fn keeps_distinct_files() {
        let dir = temp_dir();
        fs::write(dir.join("x.rs"), "fn x() {}\n").unwrap();
        fs::write(dir.join("y.rs"), "fn y() {}\n").unwrap();
        assert_eq!(
            dedup_inputs(vec![dir.join("x.rs"), dir.join("y.rs")]),
            vec![dir.join("x.rs"), dir.join("y.rs")]
        );
        cleanup(&dir);
    }
}
