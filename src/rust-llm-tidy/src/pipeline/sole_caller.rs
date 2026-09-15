//! Build MOD004's per-crate Rust and per-project C# sole-caller facts
//! for the lint phase.
//!
//! Rust inputs yield one whole-crate parse per owning crate; C# inputs
//! yield one namespace-reference index per project closure. Both
//! merge into one finding set keyed by anchor file.

use super::{RunOptions, effective_policy};
use crate::config::CompiledConfig;
use crate::input as paths;
use crate::languages::csharp::analysis::namespace_refs::NamespaceRefIndex;
use crate::project::csharp::CSharpIndex;
use crate::project::rust_crate::RustCrateIndex;
use crate::rules::lint::csharp::mod004_sole_caller as csharp_mod004;
use crate::rules::lint::rust::mod004_sole_caller;
use crate::rules::lint::sole_caller::SoleCallerFindings;
use crate::rules::registry as check;
use crate::source::ParseResult;
use ahash::AHashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Build MOD004's sole-caller findings from the shared C# parses.
///
/// Gates mirror the Rust facts minus the Cargo permission: linting
/// may run, and at least one `.cs` input must select MOD004 under its
/// policy.
///
/// The pass measures each scope alone (nearest `.csproj` plus its
/// references; project-less inputs form one loose scope). A scope
/// keeps only findings anchored at its own files and reuses the
/// refreshed [`CSharpIndex`] parses.
///
/// # Arguments
///
/// - `index` - the run's refreshed C# parse cache, when one exists
/// - `paths` - the run's resolved input paths
/// - `config` - the configuration the run loads before the lint phase,
///   or language defaults
/// - `included` - explicit CLI include set, when present
/// - `disabled` - explicit CLI exclude set
///
/// # Returns
///
/// The grouped findings when the gates pass and a C# index exists;
/// `None` otherwise.
pub(super) fn csharp_sole_caller_findings(
    index: Option<&CSharpIndex>,
    paths: &[PathBuf],
    config: Option<&CompiledConfig>,
    included: Option<&HashSet<String>>,
    disabled: &HashSet<String>,
) -> Option<SoleCallerFindings> {
    if !lints_may_run(included, disabled)
        || !selects_mod004(paths, "cs", config, included, disabled)
    {
        return None;
    }
    let index = index?;
    let scopes = index.scopes();
    let mut merged = SoleCallerFindings::new(AHashMap::new());
    for scope in scopes.list() {
        // Scope files arrive sorted, so edge order stays deterministic.
        let parses: Vec<(&Path, &ParseResult)> = scope
            .files
            .iter()
            .filter_map(|file| index.parsed(file).map(|parse| (file.as_path(), parse)))
            .collect();
        let mut findings = csharp_mod004::analyze(&NamespaceRefIndex::from_parses(parses));
        // Overlapping closures double-report; keep only the anchors
        // this scope's project owns (`None` = the loose scope).
        findings.retain_anchors(|file| scopes.owned_by(file, scope.project.as_deref()));
        merged.merge(findings);
    }
    Some(merged)
}

/// Build MOD004's sole-caller findings from one whole-crate parse per
/// owning crate.
///
/// Gates mirror the vis context: linting may run and the run options
/// permit Cargo discovery (the crate lookup also runs
/// `cargo metadata`).
///
/// At least one `.rs` input must both lint and select MOD004 under
/// its resolved per-file policy; otherwise this pass returns `None`.
///
/// # Arguments
///
/// - `paths` - the run's resolved input paths
/// - `options` - the run options, supplying the Cargo permission
/// - `config` - the configuration the run loads before the lint phase,
///   or language defaults
/// - `included` - explicit CLI include set, when present
/// - `disabled` - explicit CLI exclude set
/// - `warnings` - sink for per-crate discovery-failure warnings
///
/// # Returns
///
/// The grouped findings when the gates pass; empty when every crate
/// fails discovery, and `None` when a gate fails.
pub(super) fn sole_caller_findings(
    paths: &[PathBuf],
    options: &RunOptions,
    config: Option<&CompiledConfig>,
    included: Option<&HashSet<String>>,
    disabled: &HashSet<String>,
    warnings: &mut Vec<String>,
) -> Option<SoleCallerFindings> {
    if !lints_may_run(included, disabled) || !options.cargo_discovery {
        return None;
    }
    if !selects_mod004(paths, "rs", config, included, disabled) {
        return None;
    }
    let mut merged = SoleCallerFindings::new(AHashMap::new());
    for index in &RustCrateIndex::build_all(paths, warnings) {
        merged.merge(mod004_sole_caller::analyze(index));
    }
    Some(merged)
}

/// Whether the explicit selection still permits any lint phase.
fn lints_may_run(included: Option<&HashSet<String>>, disabled: &HashSet<String>) -> bool {
    !disabled.contains("lints")
        && included.is_none_or(|set| {
            set.contains("lints") || check::LINT_CODES.iter().any(|code| set.contains(*code))
        })
}

/// Whether one `ext` input lints and selects MOD004 under its
/// resolved per-file policy.
fn selects_mod004(
    paths: &[PathBuf],
    ext: &str,
    config: Option<&CompiledConfig>,
    included: Option<&HashSet<String>>,
    disabled: &HashSet<String>,
) -> bool {
    paths.iter().any(|path| {
        let policy = effective_policy(path, config, included, disabled);
        !policy.skip
            && paths::ext_in(path.extension().and_then(|e| e.to_str()), &[ext])
            && !policy.disabled.contains("lints")
            && !policy.disabled.contains(check::CODE_MOD004)
            && match &policy.enabled {
                Some(set) => set.contains("lints") || set.contains(check::CODE_MOD004),
                None => true,
            }
    })
}
