//! Compile run-wide hints and filter final findings against original eligibility.

use super::{RunOptions, effective_policy, file_execution};
use crate::config::{
    CompiledConfig, CompiledSymbolRule, DuplicationConfig, PerfCode, ReportingScope, SymbolAction,
    SymbolLanguage,
};
use crate::input::changed_lines::{self, ChangedLineCollection, ChangedLines};
use crate::languages::{backend_for, registry as langs};
use crate::project::documentation::DocumentationContext;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::{self, symbols};
use crate::source::ParseResult;
use core::array::from_fn;
use core::mem;
use core::ops::Range;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Immutable policy shared by both execution phases and all selected files.
pub(super) struct LintContext<'a> {
    pub(super) config: Option<&'a CompiledConfig>,
    all_lines: bool,
    pub(super) snapshots: ChangedLineCollection,
    /// Documentation signals classified from the selected paths once per run;
    /// empty for context-free callers such as buffers.
    documentation: DocumentationContext,
    rust_hints: Vec<CompiledSymbolRule>,
    csharp_hints: Vec<CompiledSymbolRule>,
    /// Explicit CLI source extensions; configuration additions remain borrowed.
    extra_source_extensions: Vec<String>,
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
        // CLI extension declarations for `duplication_source`; the filter below
        // reads them, so seed before it.
        self.extra_source_extensions.clone_from(&options.extensions);
        let scoped: Vec<_> = paths
            .iter()
            .filter(|path| {
                let policy = effective_policy(path, self.config, included, disabled);
                if !file_execution::lints_enabled(path, self, &policy) {
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

    /// Compile selected built-in families once per language and run.
    pub(super) fn new(config: Option<&'a CompiledConfig>, all_lines: bool) -> Self {
        let selected = config.map_or(PerfCode::ALL, CompiledConfig::perf_hints);
        let compile = |language| {
            let include_array = language == SymbolLanguage::Csharp
                && selected.contains(&PerfCode::ArrayInitialization);
            let mut rules = if selected.contains(&PerfCode::Capacity) {
                symbols::builtins::capacity_reminders(language)
            } else {
                Vec::with_capacity(usize::from(include_array))
            };

            if include_array {
                rules.push(symbols::builtins::array_reminder());
            }
            rules
        };

        Self {
            config,
            all_lines,
            snapshots: ChangedLineCollection::default(),
            documentation: DocumentationContext::default(),
            rust_hints: compile(SymbolLanguage::Rust),
            csharp_hints: compile(SymbolLanguage::Csharp),
            extra_source_extensions: Vec::new(),
        }
    }

    /// Classify documentation signals for the selected inputs.
    ///
    /// Call once in the mutation phase, before parallel lint execution; the
    /// lint phase reads the cached facts and never walks ancestors itself.
    pub(super) fn classify_documentation(&mut self, paths: &[PathBuf]) {
        self.documentation = DocumentationContext::build(paths);
    }

    /// Admit code profiles or explicitly declared unmapped source extensions only.
    /// Extension selection never reclassifies existing non-code profiles.
    pub(super) fn duplication_source(&self, ext: &str) -> bool {
        match langs::profile_for(ext).module_size {
            langs::ModuleSize::WholeFile | langs::ModuleSize::RustNonTest => true,
            langs::ModuleSize::NonCode => false,
            langs::ModuleSize::None => self
                .extra_source_extensions
                .iter()
                .chain(self.config.into_iter().flat_map(|config| {
                    config
                        .extension_override()
                        .iter()
                        .chain(config.extra_extensions())
                }))
                .any(|configured| configured.eq_ignore_ascii_case(ext)),
        }
    }

    /// Determine DUP001 participation without changing any transformation profile.
    pub(super) fn duplication_enabled(
        &self,
        ext: &str,
        enabled: &Option<HashSet<String>>,
        disabled: &HashSet<String>,
    ) -> bool {
        !disabled.contains("lints")
            && !disabled.contains(lint::CODE_DUPLICATION)
            && enabled
                .as_ref()
                .is_none_or(|set| set.contains("lints") || set.contains(lint::CODE_DUPLICATION))
            && self.duplication_source(ext)
    }

    /// Match only final-source runs admitted by the effective DUP001 scope.
    /// Missing snapshots have already produced a capture warning and never widen scope.
    pub(super) fn duplication(&self, path: &Path, source: &str) -> Vec<Diagnostic> {
        let eligible = if self.scope_for(lint::CODE_DUPLICATION, Severity::Reminder, None)
            == ReportingScope::All
        {
            ChangedLines::all(source)
        } else {
            let Some(snapshot) = self.snapshots.snapshots.get(path).and_then(Option::as_ref) else {
                return Vec::new();
            };
            snapshot.remap(source)
        };

        lint::dup001_duplication::check(
            source,
            &eligible,
            self.config
                .map_or_else(DuplicationConfig::default, CompiledConfig::duplication),
        )
    }

    /// Override all severities, then resolve per-rule, per-code, and severity scopes.
    pub(super) fn scope_for(
        &self,
        code: &str,
        severity: Severity,
        rule: Option<ReportingScope>,
    ) -> ReportingScope {
        self.all_lines
            .then_some(ReportingScope::All)
            .or(rule)
            .or_else(|| self.config.and_then(|config| config.scope_for(code)))
            .unwrap_or_else(|| ReportingScope::for_severity(severity))
    }

    /// Borrow configured rules independently of SYM enablement.
    pub(super) fn rules(&self) -> &[CompiledSymbolRule] {
        self.config.map_or(&[], CompiledConfig::symbol_rules)
    }

    /// Collect snapshots only for enabled lints that can emit in this language.
    pub(super) fn needs_snapshot(&self, ext: &str, disabled: &HashSet<String>) -> bool {
        let language = SymbolLanguage::for_extension(ext);
        let symbol_hint = |rule: &CompiledSymbolRule| {
            rule.applies_to_extension(ext)
                && rule.action == SymbolAction::Hint
                && !disabled.contains(rule.code)
                && self.scope_for(rule.code, rule.severity, rule.scope)
                    == ReportingScope::ChangedLines
        };
        if self.rules().iter().any(symbol_hint) || self.builtin_hints(ext).iter().any(symbol_hint) {
            return true;
        }

        let profile = langs::profile_for(ext);
        lint::LINT_CODES.iter().any(|code| {
            let supported = match *code {
                lint::CODE_SYM => false,
                lint::CODE_DUPLICATION => self.duplication_source(ext),
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
                lint::CODE_DOCUMENTATION_CONTEXT => profile.text_lints == langs::TextLints::Prose,
                lint::CODE_MOD002 | lint::CODE_LEN001 => language == Some(SymbolLanguage::Rust),
                code if code.starts_with("TEXT") => profile.text_lints != langs::TextLints::None,
                _ => language.is_some(),
            };
            // Reminder-severity lint families report on changed lines by
            // default, so the changed-line filter needs their snapshots.
            let severity = if matches!(
                *code,
                lint::CODE_PASSIVE_NARRATION
                    | lint::CODE_DUPLICATION
                    | lint::CODE_TEST_SUMMARY
                    | lint::CODE_DOCUMENTATION_CONTEXT
            ) {
                Severity::Reminder
            } else {
                Severity::Error
            };

            supported
                && !disabled.contains(*code)
                && self.scope_for(code, severity, None) == ReportingScope::ChangedLines
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
        let excluded = excluded_lines(source, &observations.lint_exclusions);
        let suppressed = |diagnostic: &Diagnostic| {
            !matches!(
                diagnostic.code,
                lint::CODE_MODULE_SIZE | lint::CODE_MISSING_MODULE_DOCS
            ) && lint::LINT_CODES
                .iter()
                .position(|code| *code == diagnostic.code)
                .is_some_and(|index| excluded[index].overlaps(diagnostic.line, diagnostic.line))
        };

        // File-context reminder: at most one per detected file, anchored to
        // the first eligible line so ordinary scope filtering cannot hide it.
        if let Some(reminder) = self.documentation_reminder(path, source, disabled, &changed) {
            diagnostics.push(reminder);
        }

        diagnostics.retain(|diagnostic| {
            self.scope_for(diagnostic.code, diagnostic.severity, None)
                .admits(diagnostic, &changed)
                && !suppressed(diagnostic)
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
                        && !suppressed(&hint.diagnostic)
                })
                .map(|hint| hint.diagnostic),
        );
    }

    /// One documentation-audience reminder for a detected file.
    ///
    /// Anchored to the first eligible nonblank line, so a change below the
    /// opening still reports and an unchanged file stays silent under
    /// changed-line scope.
    ///
    /// Detection only guesses the context; the message asks for a review
    /// instead of claiming a defect.
    fn documentation_reminder(
        &self,
        path: &Path,
        source: &str,
        disabled: &HashSet<String>,
        changed: &ChangedLines,
    ) -> Option<Diagnostic> {
        if disabled.contains(lint::CODE_DOCUMENTATION_CONTEXT) {
            return None;
        }
        let signal = self.documentation.signal(path)?;
        let all_lines = self.scope_for(lint::CODE_DOCUMENTATION_CONTEXT, Severity::Reminder, None)
            == ReportingScope::All;
        let line = first_eligible_line(source, changed, all_lines)?;
        Some(lint::text::documentation_reminder(line, &signal.reason()))
    }

    /// Reuse the lint phase's retained parse for all shared symbol policies.
    pub(super) fn observe(
        &self,
        parsed: &ParseResult,
        ext: &str,
        disabled: &HashSet<String>,
    ) -> anyhow::Result<symbols::SymbolObservations> {
        let mut observations = symbols::check_enabled(
            parsed,
            ext,
            self.rules(),
            !disabled.contains(lint::CODE_SYM),
        )?;
        if self
            .builtin_hints(ext)
            .iter()
            .any(|rule| !disabled.contains(rule.code))
        {
            observations
                .hints
                .extend(symbols::check(parsed, ext, self.builtin_hints(ext))?.hints);
        }
        Ok(observations)
    }

    /// Selected built-in rules for supported input languages.
    fn builtin_hints(&self, ext: &str) -> &[CompiledSymbolRule] {
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
    if !rules.iter().any(|rule| {
        rule.action == SymbolAction::Exclude && rule.exclude_edits && rule.applies_to(language)
    }) {
        return Ok(Vec::new());
    }
    let Some(backend) = backend_for(ext) else {
        return Ok(Vec::new());
    };

    symbols::excluded_ranges(&backend.parse(source)?, ext, rules)
}

/// Index suppression by registered lint code, scanning source line starts once.
fn excluded_lines(
    source: &str,
    ranges: &[(Range<usize>, u32)],
) -> [ChangedLines; lint::LINT_CODES.len()] {
    if ranges.is_empty() {
        return from_fn(|_| ChangedLines::empty());
    }

    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));

    from_fn(|index| {
        ChangedLines::new(
            ranges
                .iter()
                .filter(|(range, mask)| !range.is_empty() && mask & (1 << index) != 0)
                .map(|(range, _)| {
                    starts.partition_point(|start| *start <= range.start)
                        ..=starts.partition_point(|start| *start < range.end)
                }),
        )
    })
}

/// First nonblank source line admitted by `changed`, or by `all_lines`.
fn first_eligible_line(source: &str, changed: &ChangedLines, all_lines: bool) -> Option<usize> {
    for (index, text) in source.lines().enumerate() {
        if !text.trim().is_empty() && (all_lines || changed.overlaps(index + 1, index + 1)) {
            return Some(index + 1);
        }
    }
    None
}
