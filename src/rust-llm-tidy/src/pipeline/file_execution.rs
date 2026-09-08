//! Per-file execution: one mutation or one lint phase per call.

use super::effective_policy;
use super::files::{self, VisContext};
use crate::config::{CompiledConfig, MethodLengthConfig, ModuleSizeConfig};
use crate::languages::{backend_for, registry as langs};
use crate::project::csharp::CSharpIndex;
use crate::reporting::FileReport;
use crate::rules::registry as check;
use std::collections::HashSet;
use std::path::Path;

/// One file's resolved lint gate: its MOD001/LEN001 thresholds plus whether
/// the lint phase dispatches.
struct LintGate {
    /// Resolved MOD001 eligibility and counting options.
    module_size: ModuleSizeConfig,
    /// Resolved LEN001 `max_lines` threshold.
    method_length: MethodLengthConfig,
    /// Whether linting runs for the file under the active selection.
    lints_on: bool,
}

/// Process one mutation or lint phase, retaining changes and findings.
///
/// - `dry_run`: preview without writing source
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
pub(super) fn process_one(
    path: &Path,
    config: Option<&CompiledConfig>,
    cli_include: Option<&HashSet<String>>,
    cli_disabled: &HashSet<String>,
    ctx: Option<&VisContext>,
    dry_run: bool,
    phase: (Option<FileReport>, Option<&CSharpIndex>),
) -> FileReport {
    let (prior, index) = phase;
    let lint_phase = prior.is_some();
    let mut out = prior.unwrap_or_else(|| FileReport {
        path: path.to_path_buf(),
        ..FileReport::default()
    });
    let policy = effective_policy(path, config, cli_include, cli_disabled);
    if policy.skip {
        // Excluded files are never mutated or post-processed.
        return out;
    }

    let enabled = &policy.enabled;
    let disabled = &policy.disabled;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let profile = langs::profile_for(ext);
    // A fix op qualifies its file for post-processing whenever the profile
    // allows it.

    // An AST op also needs the profile's `backend` tier and a
    // backend registered in the language registry (Rust today).
    let backend = backend_for(ext);
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
        match files::fix_file(path, dry_run, profile, enabled, disabled, links_min) {
            Ok(found) => out.changes.extend(found),
            Err(e) => {
                out.fail(&e);
                return out;
            }
        }
    }

    // Reorder next (fixes ordering).
    if !lint_phase && ast_op_on("reorder") {
        match files::reorder_file(path, dry_run, disabled) {
            Ok(found) => out.changes.extend(found),
            Err(e) => {
                out.fail(&e);
                return out;
            }
        }
    }
    // Narrow visibility next (fixes misleading bare `pub` inside
    // restricted-visibility inline modules).
    if !lint_phase && ast_op_on("vis") {
        match files::vis_file(path, dry_run, ctx, disabled) {
            Ok(found) => out.changes.extend(found),
            Err(e) => {
                out.fail(&e);
                return out;
            }
        }
    }
    let gate = lint_gate(config, profile, enabled, disabled);
    if lint_phase && gate.lints_on {
        let lint_disabled = lint_disabled_set(enabled, disabled, config);
        match files::check_file(
            path,
            &lint_disabled,
            config.is_none_or(CompiledConfig::suppress_in_release_notes),
            gate.module_size,
            gate.method_length,
            index,
        ) {
            Ok(found) => out
                .diagnostics
                .extend(found.into_iter().map(|(_, diagnostic)| diagnostic)),
            Err(e) => {
                out.fail(&e);
                return out;
            }
        }
    }

    if should_post_process {
        out.processed = true;
    }
    out
}

/// Resolve the disabled lint codes for the lint phase under the active
/// selection.
fn lint_disabled_set(
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    config: Option<&CompiledConfig>,
) -> HashSet<String> {
    match enabled {
        // In whitelist mode without `lints` in the set, only whitelisted
        // lint codes should run; disable the rest.
        Some(set) if !set.contains("lints") => check::LINT_CODES
            .iter()
            .filter(|c| !set.contains(**c))
            .map(|c| c.to_string())
            .chain(disabled.iter().cloned())
            .collect(),
        // TEXT007 is opt-in: it runs only when the config enables it or
        // the selection names the code; `lints` alone does not.
        _ => {
            let opted_in = config.is_some_and(CompiledConfig::passive_narration)
                || enabled
                    .as_ref()
                    .is_some_and(|set| set.contains(check::CODE_PASSIVE_NARRATION));
            let mut codes = disabled.clone();
            if !opted_in {
                codes.insert(check::CODE_PASSIVE_NARRATION.to_string());
            }
            codes
        }
    }
}

/// Resolve the lint gate for one file: config-or-default thresholds and the
/// selection/profile conditions deciding whether `lints` runs.
fn lint_gate(
    config: Option<&CompiledConfig>,
    profile: &langs::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
) -> LintGate {
    // The non-code size opt-in admits supported data formats to MOD001 only.
    let module_size = config.map_or_else(ModuleSizeConfig::default, CompiledConfig::module_size);
    let method_length =
        config.map_or_else(MethodLengthConfig::default, CompiledConfig::method_length);
    let non_code_size_on = module_size.include_non_code
        && profile.module_size == langs::ModuleSize::NonCode
        && !disabled.contains(check::CODE_MODULE_SIZE)
        && enabled
            .as_ref()
            .is_none_or(|set| set.contains("lints") || set.contains(check::CODE_MODULE_SIZE));
    let lints_on = !disabled.contains("lints")
        && (non_code_size_on
            || match enabled {
                Some(set) => {
                    (set.contains("lints") || check::LINT_CODES.iter().any(|c| set.contains(*c)))
                        && profile.allows("lints")
                }
                None => profile.op_enabled("lints", enabled, disabled),
            });
    LintGate {
        module_size,
        method_length,
        lints_on,
    }
}
