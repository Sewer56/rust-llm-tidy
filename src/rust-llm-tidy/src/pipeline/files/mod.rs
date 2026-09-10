//! File-level checks and fixes; project-aware visibility lives in [`vis`],
//! item reordering in [`reorder`].

use crate::config::forbidden_character_rule::defaults;
use crate::config::{CompiledConfig, CompiledSymbolRule, MethodLengthConfig, ModuleSizeConfig};
use crate::input as paths;
use crate::input::file_io as io;
use crate::languages::backend_for;
use crate::languages::registry as langs;
use crate::project::csharp as csharp_index;
use crate::reporting::change as changes;
use crate::reporting::{Diagnostic, RunReport};
use crate::rules::lint as check;
use crate::source::ParseResult;
use crate::text::comments;
use crate::text::measurement::{line_marker_regions, measure};
use anyhow::Context;
pub(crate) use reorder::reorder_file;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
pub(crate) use vis::{VisContext, resolve_vis_context, vis_file};

mod reorder;
mod vis;

/// Check a single source file and return its lint diagnostics.
///
/// Returns diagnostics and skipped-check warnings for the file report.
///
/// The profile decides which passes run: parser-driven checks need a
/// registered backend; text lints source TEXT* per tier. MOD001 counts whole
/// eligible non-Rust files without requiring a parser.
///
/// DUP001 reads raw source with complete remapped queries before scope filtering.
///
/// - `path`: source file to check
/// - `disabled`: diagnostic codes to suppress
/// - `suppress_in_release_notes`: the resolved
///   `passive_narration.suppress_in_release_notes` setting; suppresses
///   TEXT007 narration markers in release and migration notes
/// - `module_size`: resolved MOD001 eligibility and counting options
/// - `method_length`: resolved LEN001 `max_lines` threshold
/// - `lint_context`: compiled hints, exclusion policies and reporting scopes
/// - `index`: refreshed C# facts and cached parses for this run
///
/// # Errors
/// Returns an error when reading source or constructing its syntax tree fails,
/// or declaration policies encounter syntax errors or missing tokens.
pub(crate) fn check_file(
    path: &Path,
    disabled: &HashSet<String>,
    suppress_in_release_notes: bool,
    module_size: ModuleSizeConfig,
    method_length: MethodLengthConfig,
    lint_context: &super::lint_context::LintContext<'_>,
    index: Option<&csharp_index::CSharpIndex>,
) -> anyhow::Result<(Vec<Diagnostic>, Vec<String>)> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let profile = langs::profile_for(ext);

    let mut diagnostics = Vec::new();
    let mut warnings = Vec::new();
    let mut observations = check::symbols::SymbolObservations::default();
    if !disabled.contains(check::CODE_DUPLICATION) && lint_context.duplication_source(ext) {
        diagnostics.extend(lint_context.duplication(path, &source));
    }
    if profile.backend
        && let Some(backend) = backend_for(ext)
    {
        let owned;
        let parsed = if let Some(parsed) = index.and_then(|i| i.parsed(path)) {
            parsed
        } else {
            owned = backend
                .parse(&source)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            &owned
        };
        diagnostics.extend(match index {
            Some(index) => backend.lint_indexed(parsed, &index.index),
            None => backend.lint(parsed),
        });
        observations = lint_context.observe(parsed, ext, disabled)?;
        if let Some(replacement) =
            forbidden_diagnostics_parsed(disabled, lint_context.config, parsed, ext)
        {
            diagnostics.retain(|d| d.code != check::CODE_FORBIDDEN_CHARACTERS);
            diagnostics.extend(replacement);
        }
        if !disabled.contains(check::CODE_SYM) {
            let text =
                check::symbols::text_regex::check(&source, ext, Some(parsed), lint_context.rules());
            observations.hints.extend(text.hints);
            warnings.extend(text.warnings);
        }
        // MOD001 is file-level: it needs the path and the threshold, which
        // never reach `LanguageBackend::lint`, so it runs at this seam.
        if profile.module_size == langs::ModuleSize::RustNonTest
            && !disabled.contains(check::CODE_MODULE_SIZE)
        {
            diagnostics.extend(check::rust::mod001_module_size::check_with_options(
                parsed,
                path,
                module_size.max_lines,
                module_size.include_in_file_tests,
                module_size.include_test_files,
                module_size.exclude_module_headers,
            ));
        }
        // LEN001 walks the retained Rust tree and consumes a config
        // threshold, so it runs at this seam like MOD001. Rust only.
        if paths::ext_in(Some(ext), &["rs"]) && !disabled.contains(check::CODE_LEN001) {
            diagnostics.extend(check::rust::len001_method_length::check(
                parsed,
                method_length.max_lines,
            ));
        }
    }

    if !profile.backend && !disabled.contains(check::CODE_SYM) {
        let text = check::symbols::text_regex::check(&source, ext, None, lint_context.rules());
        observations.hints.extend(text.hints);
        warnings.extend(text.warnings);
    }

    if (profile.module_size == langs::ModuleSize::WholeFile
        || (profile.module_size == langs::ModuleSize::NonCode && module_size.include_non_code))
        && !disabled.contains(check::CODE_MODULE_SIZE)
    {
        diagnostics.extend(check::mod001_module_size::check(
            &source,
            ext,
            module_size.max_lines,
            module_size.exclude_module_headers,
        ));
    }

    match profile.text_lints {
        langs::TextLints::Prose => diagnostics.extend(check::run_text_checks(&source, ext)),
        langs::TextLints::Lexicon => {
            diagnostics.extend(comments::text_checks(&source, ext));
        }
        langs::TextLints::Ast | langs::TextLints::None => {}
    }
    diagnostics.retain(|d| !disabled.contains(d.code));
    if !profile.backend
        && let Some(replacement) =
            forbidden_diagnostics_text(disabled, lint_context.config, &source, ext, profile)
    {
        diagnostics.retain(|d| d.code != check::CODE_FORBIDDEN_CHARACTERS);
        diagnostics.extend(replacement);
    }

    if suppress_in_release_notes && is_release_or_migration_note(path) {
        diagnostics.retain(|d| !check::is_narration_marker(d));
    }

    lint_context.filter(path, &source, observations, &mut diagnostics, disabled);

    Ok((diagnostics, warnings))
}

/// Fix table alignment, nested fence delimiters, and repeated inline links in a
/// single file.
///
/// Reads the source and applies the shared buffer transformation pipeline.
///
/// Each pass is gated by the file's [`langs::Profile`] against the active
/// rule selection. Text fixes process whole Markdown/plaintext documents or
/// parser-verified standalone line-comment groups, never arbitrary source.
///
/// Writes the result back via [`io::atomic_write`] unless `--dry-run` is
/// given.
///
/// Every edit reports a [`changes::Change`] in both dry-run and in-place
/// modes:
///
/// - fences via the transformation module's anchors
/// - tables as one per-file record ([`changes::table_changes`])
/// - link hoists as one record per before/after pair
///   ([`changes::link_changes`])
///
/// A no-op pass borrows its text back and yields no record.
///
/// # Errors
/// Returns an error when reading source or atomically writing the result fails,
/// or applicable exclusions cannot parse a complete declaration tree.
pub(crate) fn fix_file(
    path: &Path,
    dry_run: bool,
    profile: &langs::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    links_min_occurrences: usize,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<Vec<changes::Change>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let ranges = super::lint_context::protected_ranges(&source, ext, rules)?;
    let (out, change_records) = super::buffer::fix_source_protected(
        &source,
        ext,
        profile,
        enabled,
        disabled,
        links_min_occurrences,
        &ranges,
    );
    if !dry_run && out != source {
        io::atomic_write(path, &out)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(change_records)
}

/// Withhold files with matching post-processing opt-outs; fail closed on reads
/// or declaration parsing required by applicable opt-outs.
pub(super) fn post_process_inputs(
    report: &mut RunReport,
    rules: &[CompiledSymbolRule],
) -> Vec<PathBuf> {
    let mut processed = Vec::with_capacity(report.files.len());
    for file in &mut report.files {
        if !file.processed {
            continue;
        }
        let ext = file
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let exclusions: Vec<_> = rules
            .iter()
            .filter(|rule| rule.exclude_post_process && rule.applies_to_extension(ext))
            .collect();
        if exclusions.is_empty() {
            processed.push(file.path.clone());
            continue;
        }

        let protection = fs::read_to_string(&file.path)
            .map_err(anyhow::Error::from)
            .and_then(|source| {
                let backend =
                    backend_for(ext).context("post-processing exclusion needs a parser")?;
                check::symbols::excludes_post_process(&backend.parse(&source)?, ext, &exclusions)
            });

        match protection {
            Ok(false) => processed.push(file.path.clone()),
            Ok(true) => report.warnings.push(format!(
                "post-processing skipped for {}: matching declaration sets exclude_post_process",
                file.path.display()
            )),
            Err(error) => file.fail(&error),
        }
    }
    processed
}

/// Resolved TEXT009 diagnostics for a parsed backend file.
///
/// Returns `None` when disabled; callers replace backend defaults so entry
/// scopes and complete comment extraction use the same path for every policy.
fn forbidden_diagnostics_parsed(
    disabled: &HashSet<String>,
    config: Option<&CompiledConfig>,
    parsed: &ParseResult,
    ext: &str,
) -> Option<Vec<Diagnostic>> {
    if disabled.contains(check::CODE_FORBIDDEN_CHARACTERS) {
        return None;
    }
    let rules = config
        .map(CompiledConfig::forbidden_characters)
        .unwrap_or_else(|| defaults());
    Some(check::text::forbidden_characters::parsed_diagnostics(
        parsed, ext, rules,
    ))
}

/// Resolved TEXT009 diagnostics for a backend-less text file.
///
/// Returns `None` when disabled; replaces defaults
/// like [`forbidden_diagnostics_parsed`].
fn forbidden_diagnostics_text(
    disabled: &HashSet<String>,
    config: Option<&CompiledConfig>,
    source: &str,
    ext: &str,
    profile: &langs::Profile,
) -> Option<Vec<Diagnostic>> {
    if disabled.contains(check::CODE_FORBIDDEN_CHARACTERS) {
        return None;
    }
    let rules = config
        .map(CompiledConfig::forbidden_characters)
        .unwrap_or_else(|| defaults());

    let regions = match profile.text_lints {
        langs::TextLints::Prose => line_marker_regions(source, ext),
        langs::TextLints::Lexicon => comments::doc_regions(source, ext),
        _ => Vec::new(),
    };
    Some(check::text::forbidden_characters::scoped_diagnostics(
        &measure(regions),
        rules,
        profile.text_lints == langs::TextLints::Prose,
    ))
}

/// Whether `path` is a release or migration note: a `CHANGELOG*` or
/// `MIGRATION*` basename at any depth, or any path under a `releases`
/// directory.
///
/// Matching is case-insensitive and independent of the config directory.
fn is_release_or_migration_note(path: &Path) -> bool {
    fn lower(s: &OsStr) -> String {
        s.to_string_lossy().to_lowercase()
    }

    let file = path.file_name().map(lower);
    let named = file.is_some_and(|n| n.starts_with("changelog") || n.starts_with("migration"));
    named
        || path
            .components()
            .any(|c| lower(c.as_os_str()) == "releases")
}
