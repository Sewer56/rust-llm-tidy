//! Compile run-wide hints and filter final findings against original eligibility.

use super::{RunOptions, effective_policy, file_execution};
use crate::config::{
    CompiledConfig, CompiledSymbolRule, PerfHint, ReportingScope, SymbolAction, SymbolLanguage,
};
use crate::input::changed_lines::{self, ChangedLineCollection, ChangedLines};
use crate::languages::{backend_for, registry as langs};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::{self, symbols};
use crate::source::ParseResult;
use core::mem;
use core::ops::Range;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Immutable policy shared by both execution phases and all selected files.
pub(super) struct LintContext<'a> {
    pub(super) config: Option<&'a CompiledConfig>,
    pub(super) scope: Option<ReportingScope>,
    pub(super) snapshots: ChangedLineCollection,
    rust_hints: Vec<CompiledSymbolRule>,
    csharp_hints: Vec<CompiledSymbolRule>,
}

impl<'a> LintContext<'a> {
    /// Capture original eligibility before mutations, respecting Git permission.
    ///
    /// Explicit baselines are validated even without snapshot-eligible files.
    /// Read and baseline failures propagate without broadening report scope.
    pub(super) fn capture(
        &mut self,
        paths: &[PathBuf],
        options: &RunOptions,
        included: Option<&HashSet<String>>,
        disabled: &HashSet<String>,
    ) -> anyhow::Result<Vec<String>> {
        let scoped: Vec<_> = paths
            .iter()
            .filter(|path| {
                let policy = effective_policy(path, self.config, included, disabled);
                if !file_execution::lints_enabled(path, self.config, &policy) {
                    return false;
                }

                let disabled = file_execution::lint_disabled_set(
                    &policy.enabled,
                    &policy.disabled,
                    self.config,
                );
                let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
                self.needs_snapshot(ext, &disabled)
            })
            .cloned()
            .collect();

        if scoped.is_empty() {
            if let Some(reference) = options.diff_base.as_deref() {
                let directory = paths
                    .first()
                    .and_then(|path| path.parent())
                    .or_else(|| options.paths.first().map(PathBuf::as_path))
                    .unwrap_or(Path::new("."));
                changed_lines::validate_baseline(directory, reference)?;
            }
            return Ok(Vec::new());
        }
        if !options.git_changed && options.diff_base.is_none() {
            return Ok(vec!["changed-line reporting skipped: Git reads were not granted; enable git_changed or supply diff_base".into()]);
        }

        self.snapshots = changed_lines::collect(&scoped, options.diff_base.as_deref())?;
        Ok(mem::take(&mut self.snapshots.warnings))
    }

    /// Resolve legacy replacement and extra lists once per language and run.
    pub(super) fn new(config: Option<&'a CompiledConfig>, scope: Option<ReportingScope>) -> Self {
        let base = config.and_then(CompiledConfig::configured_perf_hints);
        let extra: &[PerfHint] = config.map_or(&[], CompiledConfig::extra_perf_hints);
        let compile = |language, defaults| {
            let mut rules =
                symbols::legacy::compile_legacy_hints(language, base.unwrap_or(defaults), extra);
            if language == SymbolLanguage::Csharp {
                rules.push(symbols::builtins::array_reminder());
            }
            rules
        };

        Self {
            config,
            scope,
            snapshots: ChangedLineCollection::default(),
            rust_hints: compile(
                SymbolLanguage::Rust,
                lint::rust::perf001_allocation_hints::default_hints(),
            ),
            csharp_hints: compile(
                SymbolLanguage::Csharp,
                lint::csharp::perf001_allocation_hints::default_hints(),
            ),
        }
    }

    /// Resolve the universal override before the per-rule and per-code scopes.
    pub(super) fn scope_for(
        &self,
        code: &str,
        severity: Severity,
        rule: Option<ReportingScope>,
    ) -> ReportingScope {
        self.scope
            .or(rule)
            .or_else(|| self.config.and_then(|config| config.scope_for(code)))
            .unwrap_or_else(|| ReportingScope::for_severity(severity))
    }

    /// Borrow configured rules independently of SYM001 enablement.
    pub(super) fn rules(&self) -> &[CompiledSymbolRule] {
        self.config.map_or(&[], CompiledConfig::symbol_rules)
    }

    /// Collect snapshots only for enabled lints that can emit in this language.
    pub(super) fn needs_snapshot(&self, ext: &str, disabled: &HashSet<String>) -> bool {
        let language = SymbolLanguage::for_extension(ext);
        let symbol_hint = |rule: &CompiledSymbolRule| {
            language.is_some_and(|language| rule.applies_to(language))
                && rule.action == SymbolAction::Hint
                && !disabled.contains(rule.code)
                && self.scope_for(rule.code, rule.severity, rule.scope)
                    == ReportingScope::ChangedLines
        };
        if self.rules().iter().any(symbol_hint) || self.legacy_hints(ext).iter().any(symbol_hint) {
            return true;
        }

        let profile = langs::profile_for(ext);
        lint::LINT_CODES.iter().any(|code| {
            let supported = match *code {
                lint::CODE_PERF001 | lint::CODE_PERF002 | lint::CODE_SYM001 => false,
                lint::CODE_MODULE_SIZE => {
                    matches!(
                        profile.module_size,
                        langs::ModuleSize::RustNonTest | langs::ModuleSize::WholeFile
                    ) || (profile.module_size == langs::ModuleSize::NonCode
                        && self
                            .config
                            .is_some_and(|config| config.module_size().include_non_code))
                }
                lint::CODE_MISSING_MODULE_DOCS => profile.backend,
                lint::CODE_MOD002 | lint::CODE_LEN001 => language == Some(SymbolLanguage::Rust),
                code if code.starts_with("TEXT") => profile.text_lints != langs::TextLints::None,
                _ => language.is_some(),
            };
            supported
                && !disabled.contains(*code)
                && self.scope_for(code, Severity::Error, None) == ReportingScope::ChangedLines
        })
    }

    /// Apply scopes and declaration suppression before any output consumer.
    pub(super) fn filter(
        &self,
        path: &Path,
        source: &str,
        observations: symbols::SymbolObservations,
        diagnostics: &mut Vec<Diagnostic>,
        disabled: &HashSet<String>,
    ) {
        let changed = self
            .snapshots
            .snapshots
            .get(path)
            .and_then(Option::as_ref)
            .map_or_else(ChangedLines::empty, |snapshot| snapshot.remap(source));
        let excluded = excluded_lines(source, &observations.excluded_ranges);

        diagnostics.retain(|diagnostic| {
            self.scope_for(diagnostic.code, diagnostic.severity, None)
                .admits(diagnostic, &changed)
                && (matches!(
                    diagnostic.code,
                    lint::CODE_MODULE_SIZE | lint::CODE_MISSING_MODULE_DOCS
                ) || !excluded.overlaps(diagnostic.line, diagnostic.line))
        });
        diagnostics.extend(
            observations
                .hints
                .into_iter()
                .filter(|hint| {
                    !disabled.contains(hint.diagnostic.code)
                        && self
                            .scope_for(hint.diagnostic.code, hint.diagnostic.severity, hint.scope)
                            .admits(&hint.diagnostic, &changed)
                        && !excluded.overlaps(hint.diagnostic.line, hint.diagnostic.line)
                })
                .map(|hint| hint.diagnostic),
        );
    }

    /// Reuse the lint phase's retained parse for all shared symbol policies.
    pub(super) fn observe(
        &self,
        parsed: &ParseResult,
        ext: &str,
        disabled: &HashSet<String>,
    ) -> anyhow::Result<symbols::SymbolObservations> {
        let mut observations = if disabled.contains(lint::CODE_SYM001) {
            symbols::SymbolObservations {
                excluded_ranges: symbols::excluded_ranges(parsed, ext, self.rules())?,
                ..Default::default()
            }
        } else {
            symbols::check(parsed, ext, self.rules())?
        };
        if self
            .legacy_hints(ext)
            .iter()
            .any(|rule| !disabled.contains(rule.code))
        {
            observations
                .hints
                .extend(symbols::check(parsed, ext, self.legacy_hints(ext))?.hints);
        }
        Ok(observations)
    }

    /// Resolved legacy rules for supported input languages.
    pub(super) fn legacy_hints(&self, ext: &str) -> &[CompiledSymbolRule] {
        match SymbolLanguage::for_extension(ext) {
            Some(SymbolLanguage::Rust) => &self.rust_hints,
            Some(SymbolLanguage::Csharp) => &self.csharp_hints,
            None => &[],
        }
    }
}

/// Recompute declaration byte ranges against the current transformation input.
pub(super) fn protected_ranges(
    source: &str,
    ext: &str,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<Vec<Range<usize>>> {
    let Some(language) = SymbolLanguage::for_extension(ext) else {
        return Ok(Vec::new());
    };
    if !rules
        .iter()
        .any(|rule| rule.action == SymbolAction::Exclude && rule.applies_to(language))
    {
        return Ok(Vec::new());
    }
    let Some(backend) = backend_for(ext) else {
        return Ok(Vec::new());
    };

    symbols::excluded_ranges(&backend.parse(source)?, ext, rules)
}

/// Translate protected bytes to diagnostic line coordinates in one source scan.
fn excluded_lines(source: &str, ranges: &[Range<usize>]) -> ChangedLines {
    if ranges.is_empty() {
        return ChangedLines::empty();
    }

    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));

    ChangedLines::new(
        ranges
            .iter()
            .filter(|range| !range.is_empty())
            .map(|range| {
                starts.partition_point(|start| *start <= range.start)
                    ..=starts.partition_point(|start| *start < range.end)
            }),
    )
}
