//! Acceptance tests for shared report scopes and snapshot eligibility.

use super::lint_context::LintContext;
use crate::config::{CompiledConfig, ReportingScope, load_and_compile};
use crate::input::changed_lines::{ChangedLineSnapshot, ChangedLines};
use crate::languages::backend_for;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::LINT_CODES;
use crate::rules::lint::symbols::SymbolObservations;
use core::iter::once;
use core::ops::RangeInclusive;
use core::slice::from_ref;
use rstest::rstest;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Documentation reminders anchor to the first eligible nonblank line, so a
/// change below the opening still reports and is never duplicated.
#[rstest]
#[case::changed_opening(1..=1, 1)]
#[case::changed_body(3..=3, 3)]
#[case::changed_tail(5..=5, 5)]
#[case::blank_changed_line(2..=2, 0)]
#[case::beyond_the_source(9..=9, 0)]
fn documentation_reminder_should_anchor_at_the_first_eligible_line(
    #[case] changed: RangeInclusive<usize>,
    #[case] expected: usize,
) {
    let source = "# Title\n\nIntro.\n\nDetails.\n";
    let (mut context, path) = documented_context(None, false, "docs/setup.md", source);
    snapshot(
        &mut context,
        &path,
        source,
        ChangedLines::new(once(changed)),
    );
    let mut diagnostics = Vec::new();

    context.filter(
        &path,
        source,
        SymbolObservations::default(),
        &mut diagnostics,
        &HashSet::new(),
    );

    assert_eq!(
        diagnostics.len(),
        usize::from(expected != 0),
        "{diagnostics:?}"
    );
    if expected != 0 {
        assert_eq!(diagnostics[0].code, "TEXT010");
        assert_eq!(diagnostics[0].severity, Severity::Reminder);
        assert_eq!(diagnostics[0].line, expected);
    }
}

/// Remapped eligibility drives the anchor: untouched lines keep reporting
/// after a transformation, while rewritten lines fail closed.
#[rstest]
#[case::unchanged_lines_elsewhere(
    "# Title\n\nBody.\n",
    1..=1,
    "# Title\n\nBody.\n\nExtra.\n",
    1
)]
#[case::rewritten_line("# Title\n\nOld body.\n", 3..=3, "# Title\n\nNew body.\n", 0)]
fn documentation_reminder_should_follow_remapped_eligibility(
    #[case] snapshot_source: &str,
    #[case] changed: RangeInclusive<usize>,
    #[case] transformed: &str,
    #[case] expected: usize,
) {
    let (mut context, path) = documented_context(None, false, "docs/setup.md", snapshot_source);
    snapshot(
        &mut context,
        &path,
        snapshot_source,
        ChangedLines::new(once(changed)),
    );
    let mut diagnostics = Vec::new();

    context.filter(
        &path,
        transformed,
        SymbolObservations::default(),
        &mut diagnostics,
        &HashSet::new(),
    );

    assert_eq!(
        diagnostics.len(),
        usize::from(expected != 0),
        "{diagnostics:?}"
    );
    if expected != 0 {
        assert_eq!(diagnostics[0].line, expected);
    }
}

/// Whole-file scopes report the reminder without any snapshot, and disabling
/// the rule suppresses it.
#[rstest]
#[case::all_lines_entry(None, true, false, 1)]
#[case::all_lines_config(Some("lint_scopes: {TEXT010: all}"), false, false, 1)]
#[case::changed_lines_without_snapshot(None, false, false, 0)]
#[case::disabled(None, true, true, 0)]
fn documentation_reminder_should_follow_scope_and_selection(
    #[case] yaml: Option<&str>,
    #[case] all_lines: bool,
    #[case] disabled: bool,
    #[case] expected: usize,
) {
    let source = "# Title\n\nIntro.\n";
    let config = yaml.map(|yaml| compile(yaml, &[]));
    let (context, path) = documented_context(config.as_ref(), all_lines, "docs/setup.md", source);
    let disabled: HashSet<String> = disabled.then(|| "TEXT010".to_owned()).into_iter().collect();
    let mut diagnostics = Vec::new();

    context.filter(
        &path,
        source,
        SymbolObservations::default(),
        &mut diagnostics,
        &disabled,
    );

    assert_eq!(diagnostics.len(), expected, "{diagnostics:?}");
    if expected != 0 {
        assert_eq!(diagnostics[0].line, 1);
    }
}

/// Undetected files stay silent even in an all-lines audit.
#[test]
fn documentation_reminder_should_skip_undetected_files() {
    let source = "# Notes\n\nInternal notes.\n";
    let (context, path) = documented_context(None, true, "notes.md", source);
    let mut diagnostics = Vec::new();

    context.filter(
        &path,
        source,
        SymbolObservations::default(),
        &mut diagnostics,
        &HashSet::new(),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[rstest]
#[case::moved(
    "prefix();\na();\nb();\nc();\nd();\ne();\na();\nb();\nc();\nd();\ne();\na();\nb();\nc();\nd();\ne();\n",
    2
)]
#[case::increased_copies(
    "a();\nb();\nc();\nd();\ne();\na();\nb();\nc();\nd();\ne();\na();\nb();\nc();\nd();\ne();\na();\nb();\nc();\nd();\ne();\n",
    0
)]
fn duplication_should_use_remapped_authority_without_admitting_new_copies(
    #[case] transformed: &str,
    #[case] anchor: usize,
) {
    let source = "a();\nb();\nc();\nd();\ne();\n".repeat(3);
    let path = Path::new("input.js");
    let mut context = LintContext::new(None, false);
    context.snapshots.snapshots.insert(
        path.into(),
        Some(ChangedLineSnapshot {
            changed: ChangedLines::all(&source),
            source: source.into(),
        }),
    );

    let findings = context.duplication(path, transformed);

    assert_eq!(findings.first().map_or(0, |finding| finding.line), anchor);
}

#[rstest]
#[case::qualifier(2, false)]
#[case::api_name(3, true)]
#[case::argument(4, false)]
fn filter_should_admit_only_the_reported_api_line(
    #[case] changed_line: usize,
    #[case] reported: bool,
) {
    let source = "fn f() {\n    Vec\n        ::new(\n        );\n}\n";
    let path = Path::new("input.rs");
    let config = compile(
        "perf_hints: []\nsymbol_rules: [{symbol: 'Vec::new', title: Capacity, message: reminder}]",
        &[],
    );
    let mut context = LintContext::new(Some(&config), false);
    context.snapshots.snapshots.insert(
        path.into(),
        Some(ChangedLineSnapshot {
            source: source.into(),
            changed: ChangedLines::new(once(changed_line..=changed_line)),
        }),
    );
    let parsed = backend_for("rs").unwrap().parse(source).unwrap();
    let disabled = HashSet::new();
    let observations = context.observe(&parsed, "rs", &disabled).unwrap();
    let mut diagnostics = Vec::new();

    context.filter(path, source, observations, &mut diagnostics, &disabled);

    assert_eq!(diagnostics.len(), usize::from(reported));
    if reported {
        assert_eq!(diagnostics[0].line, 3);
    }
}

#[rstest]
#[case::error(Severity::Error, true)]
#[case::warning(Severity::Warning, true)]
#[case::hint(Severity::Hint, true)]
#[case::reminder(Severity::Reminder, false)]
fn filter_should_apply_severity_defaults_without_a_snapshot(
    #[case] severity: Severity,
    #[case] reported: bool,
) {
    let context = LintContext::new(None, false);
    let mut diagnostics = vec![Diagnostic {
        title: None,
        severity,
        code: "DOC001",
        message: "finding".into(),
        line: 1,
        item_kind: "fn".into(),
        item_name: None,
    }];

    context.filter(
        Path::new("input.rs"),
        "fn f() {}",
        SymbolObservations::default(),
        &mut diagnostics,
        &HashSet::new(),
    );

    assert_eq!(!diagnostics.is_empty(), reported);
}

#[rstest]
#[case::severity_default("{}", false, None, ReportingScope::ChangedLines)]
#[case::code_override("lint_scopes: {SYM: all}", false, None, ReportingScope::All)]
#[case::entry_override(
    "lint_scopes: {SYM: all}",
    false,
    Some(ReportingScope::ChangedLines),
    ReportingScope::ChangedLines
)]
#[case::run_override(
    "lint_scopes: {SYM: changed_lines}",
    true,
    Some(ReportingScope::ChangedLines),
    ReportingScope::All
)]
fn scope_should_prioritize_run_then_entry_then_code_then_severity(
    #[case] yaml: &str,
    #[case] all_lines: bool,
    #[case] entry: Option<ReportingScope>,
    #[case] expected: ReportingScope,
) {
    let config = compile(yaml, &[]);
    let context = LintContext::new(Some(&config), all_lines);

    let scope = context.scope_for("SYM", Severity::Reminder, entry);

    assert_eq!(scope, expected);
}

#[rstest]
#[case::backendless("js", "{}", false, true)]
#[case::rust("rs", "{}", false, true)]
#[case::extra("custom", "extra_extensions: [custom]", false, true)]
#[case::all_scope("js", "lint_scopes: {DUP001: all}", false, false)]
#[case::run_all("js", "{}", true, false)]
#[case::prose("md", "{}", false, false)]
#[case::unknown("custom", "{}", false, false)]
fn snapshots_should_capture_duplication_only_for_changed_source_queries(
    #[case] ext: &str,
    #[case] yaml: &str,
    #[case] all_lines: bool,
    #[case] expected: bool,
) {
    let config = compile(yaml, &[]);
    let context = LintContext::new(Some(&config), all_lines);
    let disabled = LINT_CODES
        .iter()
        .filter(|&&code| code != "DUP001")
        .map(|code| (*code).to_owned())
        .collect();

    let actual = context.needs_snapshot(ext, &disabled);

    assert_eq!(actual, expected);
}

#[rstest]
#[case::rust("rs", "{}", &["TEXT007"], true)]
#[case::csharp("cs", "{}", &["TEXT007"], true)]
#[case::text("md", "{}", &["TEXT007"], false)]
#[case::python("py", "{}", &["TEXT007"], false)]
#[case::disabled("rs", "{}", &["SYM", "TEXT007"], false)]
#[case::empty_lists("rs", "perf_hints: []", &["TEXT007"], false)]
#[case::other_language("rs", "perf_hints: []\nsymbol_rules: [{languages: [csharp], symbol: Run, title: API, message: reminder}]", &["TEXT007"], false)]
#[case::warning_symbol("rs", "perf_hints: []\nsymbol_rules: [{symbol: run, title: API, message: warning, severity: warning}]", &["TEXT007"], false)]
#[case::text_override("md", "lint_scopes: {TEXT001: changed_lines}", &["TEXT007"], true)]
#[case::narration_markdown("md", "{}", &[], true)]
#[case::narration_python("py", "{}", &[], true)]
#[case::narration_rust("rs", "{}", &["SYM"], true)]
#[case::narration_config_disabled("md", "passive_narration: {enable: false}", &[], false)]
#[case::narration_all_lines("md", "lint_scopes: {TEXT007: all}", &[], false)]
#[case::narration_unsupported("unknown", "{}", &[], false)]
fn snapshots_should_require_an_enabled_supported_scoped_lint(
    #[case] ext: &str,
    #[case] yaml: &str,
    #[case] disabled: &[&str],
    #[case] needed: bool,
) {
    let config = compile(yaml, &[]);
    let context = LintContext::new(Some(&config), false);
    // Isolate the existing Reminder families; DUP001, TEST002, and TEXT010
    // have separate cases.
    let disabled = disabled
        .iter()
        .copied()
        .chain(["DUP001", "TEST002", "TEXT010"])
        .map(str::to_owned)
        .collect();
    let disabled = super::file_execution::lint_disabled_set(&None, &disabled, Some(&config));

    let actual = context.needs_snapshot(ext, &disabled);

    assert_eq!(actual, needed);
}

/// TEXT010 is reminder-severity and prose-only: markdown-family files need a
/// snapshot even when every other lint is disabled, source files do not.
#[rstest]
#[case::markdown("md", false, true)]
#[case::text("txt", false, true)]
#[case::source("rs", false, false)]
#[case::disabled("md", true, false)]
fn snapshots_should_track_the_documentation_reminder(
    #[case] ext: &str,
    #[case] text010_disabled: bool,
    #[case] needed: bool,
) {
    let config = compile("perf_hints: []", &[]);
    let context = LintContext::new(Some(&config), false);

    // Leave only TEXT010 selectable; the flag switches it off too.
    let mut disabled: HashSet<String> = LINT_CODES
        .iter()
        .filter(|&&code| code != "TEXT010")
        .map(|code| (*code).to_owned())
        .collect();
    if text010_disabled {
        disabled.insert("TEXT010".to_owned());
    }

    let actual = context.needs_snapshot(ext, &disabled);

    assert_eq!(actual, needed);
}

/// TEST002 is reminder-severity, so its supported languages need a snapshot
/// even when every other lint is disabled.
#[rstest]
#[case::rust("rs", false, true)]
#[case::csharp("cs", false, true)]
#[case::unsupported_language("md", false, false)]
#[case::disabled("rs", true, false)]
fn snapshots_should_track_the_test_summary_reminder(
    #[case] ext: &str,
    #[case] test002_disabled: bool,
    #[case] needed: bool,
) {
    let config = compile("perf_hints: []", &[]);
    let context = LintContext::new(Some(&config), false);

    // Leave only TEST002 selectable; the flag switches it off too.
    let mut disabled: HashSet<String> = LINT_CODES
        .iter()
        .filter(|&&code| code != "TEST002")
        .map(|code| (*code).to_owned())
        .collect();
    if test002_disabled {
        disabled.insert("TEST002".to_owned());
    }

    let actual = context.needs_snapshot(ext, &disabled);

    assert_eq!(actual, needed);
}

/// Compile isolated policy without granting Git or project discovery.
fn compile(yaml: &str, _files: &[(&str, &str)]) -> CompiledConfig {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(".rust-llm-tidy.yml");
    fs::write(&path, yaml).unwrap();

    load_and_compile(&path).unwrap()
}

/// A lint context with documentation signals for one temp file at `relative`.
fn documented_context<'a>(
    config: Option<&'a CompiledConfig>,
    all_lines: bool,
    relative: &str,
    content: &str,
) -> (LintContext<'a>, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
    let mut context = LintContext::new(config, all_lines);
    context.classify_documentation(from_ref(&path));
    (context, path)
}

/// Store `changed` eligibility for `path` against `source`.
fn snapshot(context: &mut LintContext<'_>, path: &Path, source: &str, changed: ChangedLines) {
    context.snapshots.snapshots.insert(
        path.into(),
        Some(ChangedLineSnapshot {
            source: source.into(),
            changed,
        }),
    );
}
