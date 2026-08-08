//! Pipeline orchestration: the main per-file loop, op-enabled check, and
//! post-process runner.

use super::Cli;
use crate::config::{CompiledConfig, PostProcessStep};
use crate::paths;
use anyhow::bail;
use rust_llm_tidy_lint::{Severity, check};
use std::collections::HashSet;
use std::path::PathBuf;

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
    let paths = paths::resolve_inputs(cli, &["rs", "md"])?;
    // Empty input (empty git diff, or explicit dir with no matching files)
    // is a success: config was already validated up front, and 0 files were
    // processed. post_process runs over 0 files.
    if paths.is_empty() {
        // JSON mode still owns stdout: emit `[]` so consumers always receive
        // exactly one valid JSON document when processing completes.
        if cli.json_mode() {
            crate::output::emit_diagnostics(&[])?;
        }
        return Ok(());
    }

    let multiple_files = paths.len() > 1;
    let json_mode = cli.json_mode();
    let mut error_count = 0usize;
    let mut failed = Vec::new();
    let mut processed: Vec<PathBuf> = Vec::new();
    let mut diagnostics: Vec<(PathBuf, rust_llm_tidy_lint::Diagnostic)> = Vec::new();

    // Build VisContext once for the crate-aware default in the vis step.
    // Only needed when vis could possibly run.
    let vis_may_run = cli_include.as_ref().is_none_or(|s| s.contains("vis"));
    let ctx = if vis_may_run {
        super::resolve_vis_context(&paths)
    } else {
        None
    };

    for path in &paths {
        let mut policy = config.map(|c| c.policy_for(path)).unwrap_or_default();
        if policy.skip {
            // Excluded files are never mutated or post-processed.
            continue;
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
        let should_post_process = ["tables", "fences", "links", "reorder", "vis"]
            .iter()
            .any(|op| op_enabled(op, enabled, disabled));

        // Fix table alignment first (auto-fixable formatting).
        if (op_enabled("tables", enabled, disabled)
            || op_enabled("fences", enabled, disabled)
            || op_enabled("links", enabled, disabled))
            && let Err(e) = super::fix_file(path, cli.dry_run, multiple_files, enabled, disabled)
        {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
            continue;
        }

        // Reorder/check are Rust-only operations.
        let is_rust = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "rs");
        if !is_rust {
            if should_post_process {
                processed.push(path.clone());
            }
            continue;
        }

        // Reorder next (fixes ordering).
        if op_enabled("reorder", enabled, disabled)
            && let Err(e) = super::reorder_file(path, cli.dry_run, multiple_files, disabled)
        {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
            continue;
        }
        // Narrow visibility next (fixes misleading bare `pub` inside
        // restricted-visibility inline modules).
        if op_enabled("vis", enabled, disabled)
            && let Err(e) =
                super::vis_file(path, cli.dry_run, multiple_files, ctx.as_ref(), disabled)
        {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
            continue;
        }
        // Then lints (reports remaining doc gaps).
        let lints_on = !disabled.contains("lints")
            && match enabled {
                Some(set) => {
                    set.contains("lints") || check::LINT_CODES.iter().any(|c| set.contains(*c))
                }
                None => true,
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
                            error_count += 1;
                        }
                        // Diagnostics are surfaced after the loop: either
                        // printed to stderr (plaintext) or projected to JSON.
                        if !json_mode {
                            eprintln!("{}:{}", p.display(), d);
                        }
                    }
                    diagnostics.extend(found);
                }
                Err(e) => {
                    eprintln!("error processing {}: {e:?}", path.display());
                    failed.push(path);
                    continue;
                }
            }
        }

        if should_post_process {
            processed.push(path.clone());
        }
    }

    // Emit the full JSON document on stdout before any bail (post-process,
    // processing-failure, or error-count) so consumers receive every finding
    // together with the non-zero exit code. Plaintext stays on stderr (already
    // printed above).
    if json_mode {
        crate::output::emit_diagnostics(&diagnostics)?;
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

/// Whether a single op is enabled for the current file, given the active
/// whitelist (`enabled`) and blacklist (`disabled`).
pub(crate) fn op_enabled(
    name: &str,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
) -> bool {
    match enabled {
        Some(set) => set.contains(name),
        None => !disabled.contains(name),
    }
}

// ---------------------------------------------------------------------------
// Post-process
// ---------------------------------------------------------------------------

/// Run every `post_process` step over the processed files.
///
/// For each step and each file: if `step.extensions` is non-empty, skip files
/// whose extension is not in the list; otherwise run
/// `Command::new(&step.command).args(&step.args).arg(file).output()` (no shell,
/// no injection). Returns the list of files that failed (non-zero exit or spawn
/// failure); each failure is also printed to stderr. `--dry-run` callers do not
/// invoke this function.
pub(crate) fn run_post_process(steps: &[PostProcessStep], files: &[PathBuf]) -> Vec<PathBuf> {
    let mut failed = Vec::new();
    for step in steps {
        for file in files {
            if !step.extensions.is_empty() {
                let ext_ok = file
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| step.extensions.iter().any(|x| x == e));
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
