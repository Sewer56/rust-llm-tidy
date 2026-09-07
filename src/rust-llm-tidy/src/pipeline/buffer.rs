//! Shared source-only operations used by buffer and file entry points.

use crate::SourceOptions;
use crate::languages::{backend_for, registry};
use crate::reporting::{Change, Diagnostic, SourceReport, change};
use crate::rules::transform::visibility::rust::{
    ParsedFile, collect_crate_reexports, narrow_vis_in_tree,
};
use crate::rules::{lint, transform};
use std::borrow::Cow;
use std::collections::HashSet;

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
/// - Malformed extension: `ext` is empty or contains dots, separators, or
///   whitespace
/// - Invalid link threshold: `links_min_occurrences` is zero
/// - Parse failure: an enabled parser cannot construct a syntax tree
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
    let (mut text, mut changes) = fix_source(
        source,
        ext,
        profile,
        &enabled,
        &disabled,
        options.links_min_occurrences,
    );
    let mut output = Cow::Borrowed(text.as_ref());
    let ast_enabled = |op| {
        profile.op_enabled(op, &enabled, &disabled)
            && backend_for(ext).is_some_and(|backend| backend.ast_ops().contains(&op))
    };

    if ast_enabled("reorder") {
        let (reordered, records) = reorder_source(&output, ext)?;
        if let Cow::Owned(reordered) = reordered {
            output = Cow::Owned(reordered);
        }
        changes.extend(records);
    }
    if ast_enabled("vis") {
        let parsed = ParsedFile::new("buffer.rs".into(), output.to_string())?;
        let reexports = collect_crate_reexports(core::iter::once(&parsed));
        let narrowed = narrow_vis_in_tree(&output, None, &reexports)?;
        changes.extend(change::vis_changes(&output, &narrowed));
        if let Cow::Owned(narrowed) = narrowed {
            output = Cow::Owned(narrowed);
        }
    }

    let diagnostics = if lints_enabled(profile, &enabled, &disabled) {
        let mut diagnostics = lint_source(&output, ext)?;
        diagnostics.retain(|diagnostic| {
            !disabled.contains(diagnostic.code)
                && enabled
                    .as_ref()
                    .is_none_or(|set| set.contains("lints") || set.contains(diagnostic.code))
        });
        diagnostics
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
        source: text,
        changes,
        diagnostics,
    })
}

/// Apply text fixes to prose documents or verified standalone comment runs.
///
/// Unverified source remains borrowed, regardless of explicit rule selection.
pub(super) fn fix_source<'a>(
    source: &'a str,
    ext: &str,
    profile: &registry::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    links_min_occurrences: usize,
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

    let runs = super::comment_fixes::comment_runs(source, ext, profile.prefixes);
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
                        .and_then(core::num::NonZeroU32::new);
                }
            }
            changes.extend(records);
        }
    }

    // Tables and identical link substitutions report once per file.
    let mut table_reported = false;
    let mut links_reported = HashSet::new();
    changes.retain(|record| {
        if record.kind == crate::reporting::ChangeKind::Table {
            !core::mem::replace(&mut table_reported, true)
        } else if record.kind == crate::reporting::ChangeKind::Link {
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

/// Reorder supported source and verify line preservation before returning
/// changes.
pub(super) fn reorder_source<'a>(
    source: &'a str,
    ext: &str,
) -> anyhow::Result<(Cow<'a, str>, Vec<Change>)> {
    let Some(backend) = backend_for(ext) else {
        return Ok((Cow::Borrowed(source), Vec::new()));
    };
    let parsed = backend.parse(source)?;
    let Some(permutation) = backend.reorder_permutation(&parsed)? else {
        return Ok((Cow::Borrowed(source), Vec::new()));
    };

    let output = transform::reorder::emit(&parsed, &permutation)?;
    crate::source::preservation::verify_line_preservation(source, &output)?;
    let changes = change::reorder_changes(&parsed, &permutation);
    let output = if output == source {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(output)
    };
    Ok((output, changes))
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

/// Run standalone checks using the registered AST or text extraction mechanism.
fn lint_source(source: &str, ext: &str) -> anyhow::Result<Vec<Diagnostic>> {
    let profile = registry::profile_for(ext);
    let mut diagnostics = if profile.backend
        && let Some(backend) = backend_for(ext)
    {
        backend.lint(&backend.parse(source)?)
    } else {
        Vec::new()
    };

    match profile.text_lints {
        registry::TextLints::Prose => diagnostics.extend(lint::run_text_checks(source, ext)),
        registry::TextLints::Lexicon => {
            diagnostics.extend(crate::text::comments::text_checks(source, ext))
        }
        registry::TextLints::Ast | registry::TextLints::None => {}
    }
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
