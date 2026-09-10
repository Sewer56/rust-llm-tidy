//! Shared source-only operations used by buffer and file entry points.

use crate::SourceOptions;
use crate::config::forbidden_character_rule::defaults;
use crate::config::{CompiledSymbolRule, compile_symbol_rules};
use crate::languages::{backend_for, registry};
use crate::reporting::{Change, ChangeKind, Diagnostic, SourceReport, change};
use crate::rules::transform::visibility::rust::{
    ParsedFile, collect_crate_reexports, narrow_vis_in_tree_protected,
};
use crate::rules::{lint, transform};
use crate::source::preservation;
use crate::text::comments;
use core::iter;
use core::mem;
use core::num::NonZeroU32;
use core::ops::Range;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

/// Tidy a standalone buffer without reading files, writing files, or running
/// commands.
///
/// Transformations run in pipeline order; lint findings describe the final
/// source.
/// Text fixes process Markdown/plaintext documents or parser-verified standalone
/// line-comment groups. Unsupported source and syntax-error trees skip text fixes;
/// literals, block comments, and trailing comments remain unchanged.
/// Rust visibility uses only local re-exports, and C# throw analysis uses only
/// this buffer.
/// Use [`crate::run`] for project-aware processing.
///
/// Declaration exclusions independently control lint suppression and edit
/// protection, including owned docs. Lint ranges are recomputed after edits.
///
/// Reminders have no eligible diff lines here. Set
/// [`SourceOptions::all_lines`] to `true` to audit all severities on all lines.
/// Enabled DUP001 skips analysis with a warning without that whole-buffer scope.
///
/// # Arguments
///
/// - `source`: original buffer, borrowed when final output is unchanged
/// - `ext`: language extension without a leading dot
/// - `options`: standalone rule selection and link threshold
///
/// # Example
///
/// ```rust
/// use rust_llm_tidy::{SourceOptions, tidy_source};
///
/// let options = SourceOptions {
///     include: vec!["DOC001".into()],
///     ..SourceOptions::default()
/// };
/// let result = tidy_source("pub fn load() {}\n", "rs", &options)?;
/// assert_eq!(result.diagnostics[0].code, "DOC001");
/// assert!(result.changes.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Failures use [`anyhow::Error`] with the failing operation's context.
///
/// - Unknown selection: an included or excluded rule name is not registered
/// - Malformed extension: `ext` is empty or has dots, separators, or whitespace
/// - Invalid link threshold: `links_min_occurrences` is zero
/// - Invalid text policy: `text_rules` entries must be valid usage regex hints
/// - Invalid symbol matcher: missing or conflicting matchers, a malformed literal,
///   or a regex rejected by dependency-default limits
/// - Invalid symbol hint: a title or message is blank or missing
/// - Conflicting symbol fields: selectors or constraints conflict with the target
///   or action, or exclusion controls appear on a hint
/// - Invalid exclusion codes: `exclude_lints` lists unregistered lint codes
///
/// Processing failures:
///
/// - Parse failure: an enabled parser cannot construct a syntax tree
/// - Declaration failure: applicable declaration policies encounter syntax errors
///   or missing tokens while observing lints or protecting edits
/// - Reorder failure: graph construction, permutation validation, or emission
///   fails
/// - Preservation failure: reordered source does not preserve the source line
///   multiset
/// - Visibility failure: parsing or applying the standalone visibility
///   transformation fails
pub fn tidy_source<'a>(
    source: &'a str,
    ext: &str,
    options: &SourceOptions,
) -> anyhow::Result<SourceReport<'a>> {
    super::validate_selection(&options.include, &options.exclude, &[])?;
    registry::validate_extension(ext)?;
    let text_rules = compile_symbol_rules(&options.text_rules)?;
    anyhow::ensure!(
        text_rules.iter().all(|rule| rule.is_text_regex()),
        "text_rules accepts only usage regex hints; use symbol_rules for symbol policies"
    );
    let mut rules = compile_symbol_rules(&options.symbol_rules)?;
    rules.extend(text_rules);
    anyhow::ensure!(
        options.links_min_occurrences > 0,
        "links minimum occurrences must be at least 1"
    );

    let mut enabled: Option<HashSet<String>> =
        (!options.include.is_empty()).then(|| options.include.iter().cloned().collect());
    let disabled: HashSet<String> = options.exclude.iter().cloned().collect();
    if let Some(enabled) = &mut enabled {
        enabled.retain(|operation| !disabled.contains(operation));
    }

    let profile = registry::profile_for(ext);
    let ranges = super::lint_context::protected_ranges(source, ext, &rules)?;
    let (mut text, mut changes) = fix_source_protected(
        source,
        ext,
        profile,
        &enabled,
        &disabled,
        options.links_min_occurrences,
        &ranges,
    );
    let mut output = Cow::Borrowed(text.as_ref());
    let ast_enabled = |op| {
        profile.op_enabled(op, &enabled, &disabled)
            && backend_for(ext).is_some_and(|backend| backend.ast_ops().contains(&op))
    };

    if ast_enabled("reorder") {
        let (reordered, records) = reorder_source(&output, ext, &rules)?;
        if let Cow::Owned(reordered) = reordered {
            output = Cow::Owned(reordered);
        }
        changes.extend(records);
    }
    if ast_enabled("vis") {
        let parsed = ParsedFile::new("buffer.rs".into(), output.to_string())?;
        let reexports = collect_crate_reexports(iter::once(&parsed));
        let ranges = super::lint_context::protected_ranges(&output, ext, &rules)?;
        let narrowed = narrow_vis_in_tree_protected(&output, None, &reexports, &ranges)?;
        changes.extend(change::vis_changes(&output, &narrowed));
        if let Cow::Owned(narrowed) = narrowed {
            output = Cow::Owned(narrowed);
        }
    }

    let mut warnings = Vec::new();
    let diagnostics = if lints_enabled(profile, &enabled, &disabled) {
        lint_buffer(
            &output,
            ext,
            &rules,
            &enabled,
            &disabled,
            options.all_lines,
            &mut warnings,
        )?
    } else {
        Vec::new()
    };
    if let Cow::Owned(output) = output {
        text = Cow::Owned(output);
    }
    if text.as_ref() == source {
        text = Cow::Borrowed(source);
    }
    Ok(SourceReport {
        warnings,
        source: text,
        changes,
        diagnostics,
    })
}

/// Apply text fixes to prose documents or verified standalone comment runs.
///
/// Unverified source remains borrowed, regardless of explicit rule selection.
#[cfg(test)]
pub(super) fn fix_source<'a>(
    source: &'a str,
    ext: &str,
    profile: &registry::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    links_min_occurrences: usize,
) -> (Cow<'a, str>, Vec<Change>) {
    fix_source_protected(
        source,
        ext,
        profile,
        enabled,
        disabled,
        links_min_occurrences,
        &[],
    )
}

/// Reorder supported source and verify line preservation before returning
/// changes.
pub(super) fn reorder_source<'a>(
    source: &'a str,
    ext: &str,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<(Cow<'a, str>, Vec<Change>)> {
    let Some(backend) = backend_for(ext) else {
        return Ok((Cow::Borrowed(source), Vec::new()));
    };
    let parsed = backend.parse(source)?;
    let ranges = lint::symbols::excluded_ranges(&parsed, ext, rules)?;
    let Some(mut permutation) = backend.reorder_permutation(&parsed)? else {
        return Ok((Cow::Borrowed(source), Vec::new()));
    };
    permutation.protect(&parsed, &ranges)?;

    let output = transform::reorder::emit(&parsed, &permutation)?;
    preservation::verify_line_preservation(source, &output)?;
    let changes = change::reorder_changes(&parsed, &permutation);
    let output = if output == source {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(output)
    };
    Ok((output, changes))
}

/// Fix prose or standalone comments, skipping runs overlapping protected bytes.
///
/// Ranges must refer to the current source. Protection applies only to parser-
/// owned comment runs; prose documents retain the ordinary whole-document path.
pub(super) fn fix_source_protected<'a>(
    source: &'a str,
    ext: &str,
    profile: &registry::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    links_min_occurrences: usize,
    ranges: &[Range<usize>],
) -> (Cow<'a, str>, Vec<Change>) {
    if profile.text_lints == registry::TextLints::Prose {
        return fix_text(source, profile, enabled, disabled, links_min_occurrences);
    }
    if !["tables", "fences", "links"]
        .iter()
        .any(|op| profile.op_enabled(op, enabled, disabled))
    {
        return (Cow::Borrowed(source), Vec::new());
    }

    let runs = super::comment_fixes::comment_runs_protected(source, ext, profile.prefixes, ranges);
    let mut output: Option<String> = None;
    let mut copied = 0;
    let mut changes = Vec::new();

    // Fix each verified run independently so links and fences cannot cross code.
    for run in runs {
        let (text, mut records) = fix_text(
            &source[run.bytes.clone()],
            profile,
            enabled,
            disabled,
            links_min_occurrences,
        );
        if let Cow::Owned(text) = text {
            // Copy untouched source verbatim, then splice in the rewritten comment run.
            let out = output.get_or_insert_with(|| String::with_capacity(source.len()));
            out.push_str(&source[copied..run.bytes.start]);
            out.push_str(&text);
            copied = run.bytes.end;

            // Translate run-local fence anchors back to original source lines.
            for record in &mut records {
                if let Some(line) = record.line {
                    record.line = u32::try_from(run.row)
                        .ok()
                        .and_then(|row| line.get().checked_add(row))
                        .and_then(NonZeroU32::new);
                }
            }
            changes.extend(records);
        }
    }

    // Tables and identical link substitutions report once per file.
    let mut table_reported = false;
    let mut links_reported = HashSet::new();
    changes.retain(|record| {
        if record.kind == ChangeKind::Table {
            !mem::replace(&mut table_reported, true)
        } else if record.kind == ChangeKind::Link {
            links_reported.insert(record.message.clone())
        } else {
            true
        }
    });
    if let Some(mut output) = output {
        output.push_str(&source[copied..]);
        (Cow::Owned(output), changes)
    } else {
        (Cow::Borrowed(source), changes)
    }
}

/// Run text engines on an already authorized document or comment run.
fn fix_text<'a>(
    source: &'a str,
    profile: &registry::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    links_min_occurrences: usize,
) -> (Cow<'a, str>, Vec<Change>) {
    let mut output = Cow::Borrowed(source);
    let mut changes = Vec::new();

    if profile.op_enabled("tables", enabled, disabled)
        && let Cow::Owned(after) = transform::fix_tables(&output, profile.prefixes)
    {
        changes.push(change::table_changes());
        output = Cow::Owned(after);
    }
    if profile.op_enabled("fences", enabled, disabled) {
        let result = transform::fix_fences(&output, profile.prefixes);
        if let Cow::Owned(after) = result.text {
            changes.extend(change::fence_changes(&result.anchors));
            output = Cow::Owned(after);
        }
    }
    if profile.op_enabled("links", enabled, disabled) {
        let (result, pairs) =
            transform::fix_links(&output, profile.prefixes, links_min_occurrences);
        if let Cow::Owned(after) = result {
            changes.extend(change::link_changes(&pairs));
            output = Cow::Owned(after);
        }
    }
    (output, changes)
}

/// Lint the transformed buffer, appending run warnings to `warnings`.
///
/// Call only when lint ops are enabled for the profile.
fn lint_buffer(
    source: &str,
    ext: &str,
    rules: &[CompiledSymbolRule],
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    all_lines: bool,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<Diagnostic>> {
    let context = super::lint_context::LintContext::new(None, all_lines);
    let hints_enabled = !disabled.contains(lint::CODE_SYM)
        && enabled
            .as_ref()
            .is_none_or(|set| set.contains("lints") || set.contains(lint::CODE_SYM));

    // Parser- and text-driven checks; DUP001 is separate because it needs the
    // whole buffer rather than the syntax tree.
    let (mut diagnostics, mut observations) =
        lint_source(source, ext, rules, &context, hints_enabled)?;
    if context.duplication_enabled(ext, enabled, disabled) {
        diagnostics.extend(context.duplication(Path::new("buffer"), source));
        if !all_lines {
            warnings.push("DUP001 analysis skipped: a standalone buffer has no input diff; set all_lines to audit it".into());
        }
    }

    // Drop findings the selection disabled after collection.
    diagnostics.retain(|diagnostic| {
        !disabled.contains(diagnostic.code)
            && enabled
                .as_ref()
                .is_none_or(|set| set.contains("lints") || set.contains(diagnostic.code))
    });

    // Usage-regex hints, then the same selection filter for hints.
    if hints_enabled {
        let text = lint::symbols::text_regex::check(source, ext, None, rules);
        warnings.extend(text.warnings);
        observations.hints.extend(text.hints);
    }
    observations.hints.retain(|hint| {
        enabled
            .as_ref()
            .is_none_or(|set| set.contains("lints") || set.contains(hint.diagnostic.code))
    });

    // Apply diff-scope filtering from the (bufferless) context.
    context.filter(
        Path::new("buffer"),
        source,
        observations,
        &mut diagnostics,
        disabled,
    );
    Ok(diagnostics)
}

/// Resolve the lint group and individual-code selections against language
/// capabilities.
fn lints_enabled(
    profile: &registry::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
) -> bool {
    !disabled.contains("lints")
        && match enabled {
            Some(set) => {
                profile.allows("lints")
                    && (set.contains("lints")
                        || lint::LINT_CODES.iter().any(|code| set.contains(*code)))
            }
            None => profile.op_enabled("lints", enabled, disabled),
        }
}

/// Run standalone checks using the registered AST or text extraction mechanism.
fn lint_source(
    source: &str,
    ext: &str,
    rules: &[CompiledSymbolRule],
    context: &super::lint_context::LintContext<'_>,
    hints_enabled: bool,
) -> anyhow::Result<(Vec<Diagnostic>, lint::symbols::SymbolObservations)> {
    let profile = registry::profile_for(ext);
    let mut observations = lint::symbols::SymbolObservations::default();
    let mut diagnostics = if profile.backend
        && let Some(backend) = backend_for(ext)
    {
        let parsed = backend.parse(source)?;
        let mut diagnostics = backend.lint(&parsed);
        diagnostics.retain(|diagnostic| diagnostic.code != lint::CODE_FORBIDDEN_CHARACTERS);
        diagnostics.extend(lint::text::forbidden_characters::parsed_diagnostics(
            &parsed,
            ext,
            defaults(),
        ));
        observations = lint::symbols::check_enabled(&parsed, ext, rules, hints_enabled)?;
        if hints_enabled {
            observations
                .hints
                .extend(context.observe(&parsed, ext, &HashSet::new())?.hints);
        }
        diagnostics
    } else {
        Vec::new()
    };

    match profile.text_lints {
        registry::TextLints::Prose => diagnostics.extend(lint::run_text_checks(source, ext)),
        registry::TextLints::Lexicon => diagnostics.extend(comments::text_checks(source, ext)),
        registry::TextLints::Ast | registry::TextLints::None => {}
    }
    Ok((diagnostics, observations))
}

#[cfg(test)]
mod protection_tests {
    use super::*;

    #[test]
    fn fix_source_should_preserve_owned_comments_and_fix_siblings() {
        let source = "/// | A | B |\n/// |---|---|\n/// | long | x |\nfn keep() {}\n/// | A | B |\n/// |---|---|\n/// | long | x |\nfn change() {}\n";
        let parsed = backend_for("rs").unwrap().parse(source).unwrap();
        let protected = crate::source::symbols::declarations(&parsed, "rs")
            .unwrap()
            .into_iter()
            .find(|item| item.path.as_ref() == "keep")
            .unwrap()
            .bytes;
        let enabled = Some(HashSet::from(["tables".into()]));
        let disabled = HashSet::new();

        let (output, changes) = fix_source_protected(
            source,
            "rs",
            registry::profile_for("rs"),
            &enabled,
            &disabled,
            2,
            core::slice::from_ref(&protected),
        );

        assert!(output.starts_with(&source[protected]));
        assert_ne!(&*output, source);
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn fix_source_should_match_original_when_ranges_empty() {
        let source = "/// | A | B |\n/// |---|---|\n/// | long | x |\nfn change() {}\n";
        let enabled = Some(HashSet::from(["tables".into()]));
        let disabled = HashSet::new();
        let profile = registry::profile_for("rs");

        let original = fix_source(source, "rs", profile, &enabled, &disabled, 2);
        let protected = fix_source_protected(source, "rs", profile, &enabled, &disabled, 2, &[]);

        assert_eq!(protected.0, original.0);
        assert_eq!(protected.1, original.1);
    }
}
