//! Pipeline orchestration: the main per-file loop, per-file op gating, and
//! post-process runner.
//!
//! Files are processed independently of each other, so the per-file work is
//! run in parallel with rayon.
//!
//! Each task buffers its plaintext lines and results; a sequential pass
//! immediately after re-emits them in input order, keeping stderr and JSON
//! output byte-identical to a single-threaded run.
//!
//! That buffering is the price of deterministic ordering: nothing is printed
//! until every file finishes, so a huge run holds all output (plus per-file
//! results) in memory before the replay pass.
//!
//! Streaming would interleave lines across threads and break byte-identical
//! output, which JSON consumers and diffing depend on.

use super::{Cli, VisContext};
use crate::config::{CompiledConfig, PostProcessStep};
use crate::paths;
use anyhow::bail;
use rayon::prelude::*;
use rust_llm_tidy_lint::{Severity, check};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Per-file processing (parallel)
// ---------------------------------------------------------------------------

/// Per-file accumulation returned by one parallel task.
///
/// Plaintext stderr lines are buffered (`printed`) instead of emitted inside
/// the task so the replay pass can print them in input order; the structure
/// mirrors the old inline loop's aggregation targets exactly.
struct PerFileOut {
    changes: Vec<(PathBuf, crate::changes::Change)>,
    diagnostics: Vec<(PathBuf, rust_llm_tidy_lint::Diagnostic)>,
    /// Plaintext stderr lines in the original loop's emission order.
    printed: Vec<String>,
    error_count: usize,
    /// True when any op failed; the file then skips the rest of its ops and is
    /// never recorded as processed.
    failed: bool,
    /// True when the file completed with at least one mutate-capable op
    /// enabled and is eligible for `post_process`.
    processed: bool,
}

impl PerFileOut {
    /// Record one op's dry-run change records: buffer plaintext lines and
    /// retain them for the unified output document.
    fn record_changes(&mut self, path: &Path, found: Vec<crate::changes::Change>, json_mode: bool) {
        for change in &found {
            if !json_mode {
                self.printed.push(format!("{}:{}", path.display(), change));
            }
        }
        self.changes
            .extend(found.into_iter().map(|c| (path.to_path_buf(), c)));
    }

    /// Mark an op failure: buffer the error line and stop processing this file
    /// (mirrors the old loop's `eprintln!` + `failed.push` + `continue`).
    fn fail(&mut self, path: &Path, err: &anyhow::Error) {
        self.printed
            .push(format!("error processing {}: {err:?}", path.display()));
        self.failed = true;
    }
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// The single default pipeline: resolve inputs, iterate files, run every op
/// that is enabled for each file, then post-process.
pub(crate) fn run_pipeline(
    cli: &Cli,
    config: Option<&CompiledConfig>,
    cli_include: Option<&HashSet<String>>,
    cli_disabled: &HashSet<String>,
) -> anyhow::Result<()> {
    // Admission is decided once per run: the config `extensions:`
    // replacement or the registry defaults, plus `extra_extensions:` and
    // `--extension`.
    let allowed = crate::langs::allowed_extensions(config, cli);
    let paths = dedup_inputs(paths::resolve_inputs(cli, &allowed)?);
    // Empty input (empty git diff, or explicit dir with no matching files)
    // is a success: config was already validated up front, and 0 files were
    // processed. post_process runs over 0 files.
    if paths.is_empty() {
        // JSON mode still owns stdout: emit `[]` so consumers always receive
        // exactly one valid JSON document when processing completes.
        if cli.json_mode() {
            crate::output::emit_json(&[], &[])?;
        }
        return Ok(());
    }

    let json_mode = cli.json_mode();
    let mut error_count = 0usize;
    let mut failed = Vec::new();
    let mut processed: Vec<PathBuf> = Vec::new();
    let mut diagnostics: Vec<(PathBuf, rust_llm_tidy_lint::Diagnostic)> = Vec::new();
    let mut changes: Vec<(PathBuf, crate::changes::Change)> = Vec::new();

    // Build VisContext once for the crate-aware default in the vis step.
    // Only needed when vis could possibly run.
    let vis_may_run = cli_include.as_ref().is_none_or(|s| s.contains("vis"));
    let ctx = if vis_may_run {
        super::resolve_vis_context(&paths)
    } else {
        None
    };

    // Parallel only pays once work exceeds rayon's ~0.7ms pool overhead.
    let parallelize = should_parallelize(&paths);

    let map_file = |path: &PathBuf| {
        process_one(
            path,
            config,
            cli_include,
            cli_disabled,
            ctx.as_ref(),
            cli.dry_run,
            json_mode,
        )
    };
    let results: Vec<PerFileOut> = if parallelize {
        paths.par_iter().map(map_file).collect()
    } else {
        paths.iter().map(map_file).collect()
    };

    // Sequential replay: emit plaintext lines in input order, then fold each
    // file's results into the aggregate collections and counts.
    for (path, out) in paths.iter().zip(results) {
        for line in &out.printed {
            eprintln!("{line}");
        }
        error_count += out.error_count;
        changes.extend(out.changes);
        diagnostics.extend(out.diagnostics);
        if out.failed {
            failed.push(path.clone());
        }
        if out.processed {
            processed.push(path.clone());
        }
    }

    // Emit the full JSON document on stdout before any bail (post-process,
    // processing-failure, or error-count) so consumers receive every finding
    // and change record together with the non-zero exit code.
    //
    // Plaintext stays on stderr (already printed above).
    if json_mode {
        crate::output::emit_json(&diagnostics, &changes)?;
    }

    if let Some(c) = config
        && !cli.dry_run
    {
        let pp_failed = run_post_process(c.post_process_steps(), &processed);
        if !pp_failed.is_empty() {
            bail!("post_process failed on {} file(s)", pp_failed.len());
        }
    }

    if !failed.is_empty() {
        bail!("failed to process {} file(s)", failed.len());
    }

    if error_count > 0 {
        bail!("found {} error(s)", error_count);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Post-process
// ---------------------------------------------------------------------------

/// Run every `post_process` step over the processed files.
///
/// For each step and each file: if `step.extensions` is non-empty, skip files
/// whose extension is not in the list; otherwise run
/// `Command::new(&step.command).args(&step.args).arg(file).output()` (no shell,
/// no injection).
///
/// Returns the list of files that failed (non-zero exit or spawn failure);
/// each failure is also printed to stderr. `--dry-run` callers do not invoke
/// this function.
pub(crate) fn run_post_process(steps: &[PostProcessStep], files: &[PathBuf]) -> Vec<PathBuf> {
    let mut failed = Vec::new();
    for step in steps {
        let exts: Vec<&str> = step.extensions.iter().map(String::as_str).collect();
        for file in files {
            if !step.extensions.is_empty() {
                let ext_ok = crate::paths::ext_in(file.extension().and_then(|e| e.to_str()), &exts);
                if !ext_ok {
                    continue;
                }
            }
            let output = std::process::Command::new(&step.command)
                .args(&step.args)
                .arg(file)
                .output();
            match output {
                Ok(out) if out.status.success() => {}
                Ok(out) => {
                    eprintln!(
                        "post_process `{}` failed on {}: {}",
                        step.command,
                        file.display(),
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                    failed.push(file.clone());
                }
                Err(e) => {
                    eprintln!(
                        "post_process `{}` failed to spawn on {}: {e}",
                        step.command,
                        file.display()
                    );
                    failed.push(file.clone());
                }
            }
        }
    }
    failed
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
/// measured workloads (single-threaded vs 32-thread runs). Either can float
/// ±50% before regret exceeds 0.5ms, so they are not sensitive.
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
    /// `1000` is markdown (the baseline); anything cheaper than markdown can
    /// be added below it. Non-Rust inputs are text-tier scans, so they fall
    /// back to the markdown weight.
    fn byte_weight(ext: Option<&str>) -> u64 {
        if crate::paths::ext_in(ext, &["rs"]) {
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
/// Unix a `(dev, ino)` key additionally catches hardlinks, which
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
            {
                use std::os::unix::fs::MetadataExt;
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

/// Process a single file: run every enabled op in the canonical order
/// (fix, reorder, vis, lints), buffering results and plaintext lines.
///
/// Shared state is read-only; each file mutates only its own path (atomic
/// write), so safe to run on one rayon thread per file.
///
/// Inputs were deduped before dispatch, so no two tasks touch the same inode
/// even under aliases.
fn process_one(
    path: &Path,
    config: Option<&CompiledConfig>,
    cli_include: Option<&HashSet<String>>,
    cli_disabled: &HashSet<String>,
    ctx: Option<&VisContext>,
    dry_run: bool,
    json_mode: bool,
) -> PerFileOut {
    let mut out = PerFileOut {
        changes: Vec::new(),
        diagnostics: Vec::new(),
        printed: Vec::new(),
        error_count: 0,
        failed: false,
        processed: false,
    };
    let mut policy = config.map(|c| c.policy_for(path)).unwrap_or_default();
    if policy.skip {
        // Excluded files are never mutated or post-processed.
        return out;
    }
    // CLI --include overrides the config mode for this run.
    if let Some(include) = cli_include {
        policy.enabled = Some(include.clone());
        policy.disabled.clear();
    }
    // CLI --exclude is additive and must remain in the disabled set so
    // lint-code exclusions survive whitelist mode.
    if !cli_disabled.is_empty() {
        policy.disabled.extend(cli_disabled.iter().cloned());
        if let Some(set) = &mut policy.enabled {
            set.retain(|r| !cli_disabled.contains(r));
        }
    }

    let enabled = &policy.enabled;
    let disabled = &policy.disabled;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let profile = crate::langs::profile_for(ext);
    // A fix op qualifies its file for post-processing whenever the profile
    // allows it; an AST op additionally needs the profile's `backend` tier
    // and a backend registered in the language registry (Rust today).
    let backend = rust_llm_tidy_lang::backend_for(ext);
    let ast_op_on = |op: &str| {
        profile.backend
            && profile.op_enabled(op, enabled, disabled)
            && backend.is_some_and(|b| b.ast_ops().contains(&op))
    };
    let should_post_process = ["tables", "fences", "links"]
        .iter()
        .any(|op| profile.op_enabled(op, enabled, disabled))
        || ["reorder", "vis"].iter().any(|op| ast_op_on(op));

    // Fix auto-fixable formatting (tables, fences, links) via fix_file.
    if profile.op_enabled("tables", enabled, disabled)
        || profile.op_enabled("fences", enabled, disabled)
        || profile.op_enabled("links", enabled, disabled)
    {
        // Resolve the link-hoist threshold by the file's extension (1 when no
        // config), so a single per-file value reaches fix_file.
        let links_min = match config {
            Some(c) => c.links_min_occurrences_for(ext),
            None => 1,
        };
        match super::fix_file(path, dry_run, profile, enabled, disabled, links_min) {
            Ok(found) => out.record_changes(path, found, json_mode),
            Err(e) => {
                out.fail(path, &e);
                return out;
            }
        }
    }

    // Reorder next (fixes ordering).
    if ast_op_on("reorder") {
        match super::reorder_file(path, dry_run, disabled) {
            Ok(found) => out.record_changes(path, found, json_mode),
            Err(e) => {
                out.fail(path, &e);
                return out;
            }
        }
    }
    // Narrow visibility next (fixes misleading bare `pub` inside
    // restricted-visibility inline modules).
    if ast_op_on("vis") {
        match super::vis_file(path, dry_run, ctx, disabled) {
            Ok(found) => out.record_changes(path, found, json_mode),
            Err(e) => {
                out.fail(path, &e);
                return out;
            }
        }
    }
    // Then lints (reports remaining doc gaps); a profile that allows no
    // `lints` op skips the pass entirely.
    let lints_on = !disabled.contains("lints")
        && match enabled {
            Some(set) => {
                (set.contains("lints") || check::LINT_CODES.iter().any(|c| set.contains(*c)))
                    && profile.allows("lints")
            }
            None => profile.op_enabled("lints", enabled, disabled),
        };
    if lints_on {
        // In whitelist mode without `lints` in the set, only whitelisted
        // lint codes should run; disable the rest.
        let lint_disabled: HashSet<String> = match enabled {
            Some(set) if !set.contains("lints") => check::LINT_CODES
                .iter()
                .filter(|c| !set.contains(**c))
                .map(|c| c.to_string())
                .chain(disabled.iter().cloned())
                .collect(),
            _ => disabled.clone(),
        };
        match super::check_file(path, &lint_disabled) {
            Ok(found) => {
                for (p, d) in &found {
                    if matches!(d.severity, Severity::Error) {
                        out.error_count += 1;
                    }
                    // Diagnostics are surfaced in the replay pass: either
                    // printed to stderr (plaintext) or projected to JSON.
                    if !json_mode {
                        out.printed.push(format!("{}:{}", p.display(), d));
                    }
                }
                out.diagnostics.extend(found);
            }
            Err(e) => {
                out.fail(path, &e);
                return out;
            }
        }
    }

    if should_post_process {
        out.processed = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::dedup_inputs;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

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
