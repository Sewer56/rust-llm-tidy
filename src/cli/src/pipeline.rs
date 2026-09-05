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
//! Hint-severity lines buffer apart and replay as their own group after
//! every other buffered line.
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
/// mirrors the old inline loop's aggregation targets, plus the `hints`
/// replay group.
struct PerFileOut {
    changes: Vec<(PathBuf, crate::changes::Change)>,
    diagnostics: Vec<(PathBuf, rust_llm_tidy_lint::Diagnostic)>,
    /// Plaintext stderr lines in the original loop's emission order.
    printed: Vec<String>,
    /// Hint-severity plaintext lines, kept out of `printed` so the
    /// replay pass can group them after every other buffered line.
    hints: Vec<String>,
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

    /// Record one file's lint findings: count gating errors, buffer plaintext
    /// lines, and retain the findings for the unified output document.
    ///
    /// Only `Severity::Error` findings count toward `error_count`, so
    /// hints and warnings never fail a run on their own. In text mode
    /// hint lines wait in `hints` for their separate replay group;
    /// in JSON mode every finding reaches the document through
    /// `diagnostics`.
    fn record_diagnostics(
        &mut self,
        found: Vec<(PathBuf, rust_llm_tidy_lint::Diagnostic)>,
        json_mode: bool,
    ) {
        for (p, d) in &found {
            if matches!(d.severity, Severity::Error) {
                self.error_count += 1;
            }
            if !json_mode {
                let line = format!("{}:{}", p.display(), d);
                if matches!(d.severity, Severity::Hint) {
                    self.hints.push(line);
                } else {
                    self.printed.push(line);
                }
            }
        }
        self.diagnostics.extend(found);
    }

    /// Mark an op failure: buffer the error line and stop processing this file
    /// (mirrors the old loop's `eprintln!` + `failed.push` + `continue`).
    fn fail(&mut self, path: &Path, err: &anyhow::Error) {
        self.printed
            .push(format!("error processing {}: {err:?}", path.display()));
        self.failed = true;
        self.processed = false;
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

    let lints_may_run = !cli_disabled.contains("lints")
        && cli_include.is_none_or(|set| {
            set.contains("lints") || check::LINT_CODES.iter().any(|code| set.contains(*code))
        });
    let mut csharp = if lints_may_run {
        Some(crate::csharp_index::CSharpIndex::build(&paths)?)
    } else {
        None
    };
    let map_file = |path: &PathBuf| {
        process_one(
            path,
            config,
            cli_include,
            cli_disabled,
            ctx.as_ref(),
            (cli.dry_run, json_mode),
            (None, None),
        )
    };
    let results: Vec<PerFileOut> = if parallelize {
        paths.par_iter().map(map_file).collect()
    } else {
        paths.iter().map(map_file).collect()
    };

    // Finish mutations before any indexed lint consumes cross-file facts.
    if let Some(index) = &mut csharp {
        index.refresh(&paths);
    }
    let lint_file = |(path, out): (&PathBuf, PerFileOut)| {
        if out.failed {
            return out;
        }
        process_one(
            path,
            config,
            cli_include,
            cli_disabled,
            ctx.as_ref(),
            (cli.dry_run, json_mode),
            (Some(out), csharp.as_ref()),
        )
    };
    let results: Vec<_> = if parallelize {
        paths
            .par_iter()
            .zip(results.into_par_iter())
            .map(lint_file)
            .collect()
    } else {
        paths.iter().zip(results).map(lint_file).collect()
    };

    // Sequential replay: emit plaintext lines in input order with hints
    // as their own trailing group, then fold each file's results into the
    // aggregate collections and counts.
    for line in replay_lines(&results) {
        eprintln!("{line}");
    }

    for (path, out) in paths.iter().zip(results) {
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

/// Process one mutation or lint phase, buffering results and plaintext lines.
///
/// - `output_mode`: dry-run and JSON-output switches
/// - `phase`: prior mutation output and optional refreshed C# index
///
/// An absent prior output selects mutations. A present output selects linting.
/// Refresh shared facts after all mutations and before dispatching lint phases.
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
    output_mode: (bool, bool),
    phase: (
        Option<PerFileOut>,
        Option<&crate::csharp_index::CSharpIndex>,
    ),
) -> PerFileOut {
    let (dry_run, json_mode) = output_mode;
    let (prior, index) = phase;
    let lint_phase = prior.is_some();
    let mut out = prior.unwrap_or_else(|| PerFileOut {
        changes: Vec::new(),
        diagnostics: Vec::new(),
        printed: Vec::new(),
        hints: Vec::new(),
        error_count: 0,
        failed: false,
        processed: false,
    });
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
    if !lint_phase
        && (profile.op_enabled("tables", enabled, disabled)
            || profile.op_enabled("fences", enabled, disabled)
            || profile.op_enabled("links", enabled, disabled))
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
    if !lint_phase && ast_op_on("reorder") {
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
    if !lint_phase && ast_op_on("vis") {
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
    if lint_phase && lints_on {
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
        match super::check_file(path, &lint_disabled, index) {
            Ok(found) => out.record_diagnostics(found, json_mode),
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

/// The plaintext replay order: every file's main lines in input order, then
/// every hint line.
///
/// Hints replay as their own trailing group so they stay visible without
/// interleaving with errors, warnings, and change records.
fn replay_lines(results: &[PerFileOut]) -> impl Iterator<Item = &str> {
    results
        .iter()
        .flat_map(|out| out.printed.iter())
        .chain(results.iter().flat_map(|out| out.hints.iter()))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::dedup_inputs;
    use rust_llm_tidy_lint::{Diagnostic, Severity};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    /// A read failure between phases revokes post-processing eligibility.
    /// Successful linting retains eligibility even when diagnostics report errors.
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
            let mutated = super::process_one(
                &path,
                None,
                Some(&included),
                &disabled,
                None,
                (false, true),
                (None, None),
            );
            assert!(mutated.processed, "{label}");
            if remove {
                fs::remove_file(&path).unwrap();
            }

            let linted = super::process_one(
                &path,
                None,
                Some(&included),
                &disabled,
                None,
                (false, true),
                (Some(mutated), None),
            );

            assert_eq!(linted.failed, remove, "{label}");
            assert_eq!(linted.processed, !remove, "{label}");
            assert_eq!(linted.error_count > 0, errors, "{label}");
        }

        cleanup(&dir);
    }

    /// `record_diagnostics` counts only error findings toward the gating
    /// error count and routes hint lines to the separate replay group.
    ///
    /// This is the exit-policy seam for hints: hint-only and
    /// hint-plus-warning mixes keep `error_count` at zero, so the
    /// unchanged `error_count > 0` bail exits 0.
    ///
    /// Adding an error counts it once and fails the run through that error
    /// alone.
    #[test]
    fn record_diagnostics_should_count_only_errors_and_group_hints() {
        for (label, severities, expected_errors, expected_hints) in [
            ("hint_only", vec![Severity::Hint], 0, 1),
            (
                "hint_and_warning",
                vec![Severity::Hint, Severity::Warning],
                0,
                1,
            ),
            (
                "hint_and_error",
                vec![Severity::Hint, Severity::Error],
                1,
                1,
            ),
        ] {
            let found: Vec<_> = severities
                .into_iter()
                .enumerate()
                .map(|(i, severity)| finding(severity, i + 1))
                .collect();
            let expected_total = found.len();

            let mut out = empty_out();
            out.record_diagnostics(found, false);

            assert_eq!(out.error_count, expected_errors, "{label}");
            assert_eq!(out.hints.len(), expected_hints, "{label}");
            assert_eq!(
                out.printed.len(),
                expected_total - expected_hints,
                "{label}: non-hint lines keep the main output group"
            );
            // Every buffered hint line is a hint-shaped line, and no
            // hint line leaks into the main group.
            assert!(
                out.hints.iter().all(|l| l.contains("hint[")),
                "{label}: {:#?}",
                out.hints
            );
            assert!(
                !out.printed.iter().any(|l| l.contains("hint[")),
                "{label}: {:#?}",
                out.printed
            );
            // The unified document still receives every finding.
            assert_eq!(out.diagnostics.len(), expected_total, "{label}");
        }
    }

    /// In JSON mode a hint still reaches the unified output document
    /// (`diagnostics`) and buffers no plaintext line in either group.
    #[test]
    fn record_diagnostics_should_keep_hints_in_the_json_document_only() {
        let mut out = empty_out();
        out.record_diagnostics(vec![finding(Severity::Hint, 3)], true);

        assert_eq!(out.error_count, 0);
        assert!(out.printed.is_empty() && out.hints.is_empty());
        assert_eq!(out.diagnostics.len(), 1);
        assert!(matches!(out.diagnostics[0].1.severity, Severity::Hint));
    }

    /// The replay emits every file's non-hint lines in input order, then
    /// every hint line, so hints form the trailing group.
    #[test]
    fn replay_lines_should_print_hints_as_the_last_group() {
        let mut first = empty_out();
        first.printed.push("a.rs:1: error[DOC001]: e".to_string());
        first.hints.push("a.rs:2: hint[DOC999]: h1".to_string());
        let mut second = empty_out();
        second
            .printed
            .push("b.rs:1: success[FIX]: fixed".to_string());
        second.hints.push("b.rs:3: hint[DOC999]: h2".to_string());

        let results = [first, second];
        let lines: Vec<&str> = super::replay_lines(&results).collect();

        assert_eq!(
            lines,
            vec![
                "a.rs:1: error[DOC001]: e",
                "b.rs:1: success[FIX]: fixed",
                "a.rs:2: hint[DOC999]: h1",
                "b.rs:3: hint[DOC999]: h2",
            ]
        );
    }

    /// One finding with the given severity and 1-based line number.
    fn finding(severity: Severity, line: usize) -> (PathBuf, Diagnostic) {
        (
            PathBuf::from("src/lib.rs"),
            Diagnostic {
                severity,
                code: "DOC999",
                message: format!("finding {line}"),
                line,
                item_kind: "fn".to_string(),
                item_name: None,
            },
        )
    }

    /// A fresh per-file accumulator for direct recording tests.
    fn empty_out() -> super::PerFileOut {
        super::PerFileOut {
            changes: Vec::new(),
            diagnostics: Vec::new(),
            printed: Vec::new(),
            hints: Vec::new(),
            error_count: 0,
            failed: false,
            processed: false,
        }
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
