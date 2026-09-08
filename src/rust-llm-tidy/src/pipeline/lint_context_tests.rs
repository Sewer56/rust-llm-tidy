//! Acceptance tests for shared report scopes and snapshot eligibility.

use super::lint_context::LintContext;
use crate::config::{CompiledConfig, ReportingScope, load_and_compile};
use crate::input::changed_lines::{ChangedLineSnapshot, ChangedLines};
use crate::languages::backend_for;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::symbols::SymbolObservations;
use rstest::rstest;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
        "symbol_rules: [{symbol: 'Vec::new', message: reminder}]",
        &[],
    );
    let mut context = LintContext::new(Some(&config), None);
    context.snapshots.snapshots.insert(
        path.into(),
        Some(ChangedLineSnapshot {
            source: source.into(),
            changed: ChangedLines::new([changed_line..=changed_line]),
        }),
    );
    let parsed = backend_for("rs").unwrap().parse(source).unwrap();
    let disabled = HashSet::from(["PERF001".to_string()]);
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
    let context = LintContext::new(None, None);
    let mut diagnostics = vec![Diagnostic {
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
#[case::severity_default("{}", None, None, ReportingScope::ChangedLines)]
#[case::code_override("lint_scopes: {SYM001: all}", None, None, ReportingScope::All)]
#[case::entry_override(
    "lint_scopes: {SYM001: all}",
    None,
    Some(ReportingScope::ChangedLines),
    ReportingScope::ChangedLines
)]
#[case::run_override(
    "lint_scopes: {SYM001: changed_lines}",
    Some(ReportingScope::All),
    Some(ReportingScope::ChangedLines),
    ReportingScope::All
)]
fn scope_should_prioritize_run_then_entry_then_code_then_severity(
    #[case] yaml: &str,
    #[case] run: Option<ReportingScope>,
    #[case] entry: Option<ReportingScope>,
    #[case] expected: ReportingScope,
) {
    let config = compile(yaml, &[]);
    let context = LintContext::new(Some(&config), run);

    let scope = context.scope_for("SYM001", Severity::Reminder, entry);

    assert_eq!(scope, expected);
}

#[rstest]
#[case::rust("rs", "{}", &[], true)]
#[case::csharp("cs", "{}", &[], true)]
#[case::text("md", "{}", &[], false)]
#[case::python("py", "{}", &[], false)]
#[case::disabled("rs", "{}", &["PERF001"], false)]
#[case::empty_lists("rs", "perf_hints: []", &[], false)]
#[case::other_language("rs", "perf_hints: []\nsymbol_rules: [{language: csharp, symbol: Run, message: reminder}]", &[], false)]
#[case::warning_symbol("rs", "perf_hints: []\nsymbol_rules: [{symbol: run, message: warning, severity: warning}]", &[], false)]
#[case::text_override("md", "lint_scopes: {TEXT001: changed_lines}", &[], true)]
fn snapshots_should_require_an_enabled_supported_scoped_lint(
    #[case] ext: &str,
    #[case] yaml: &str,
    #[case] disabled: &[&str],
    #[case] needed: bool,
) {
    let config = compile(yaml, &[]);
    let context = LintContext::new(Some(&config), None);
    let disabled = disabled.iter().map(|code| (*code).to_owned()).collect();

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
