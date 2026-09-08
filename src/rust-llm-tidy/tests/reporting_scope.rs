//! Public reporting boundaries without ambient Git permission.

use rstest::rstest;
use rust_llm_tidy::config::{ReportingScope, load_and_compile};
use rust_llm_tidy::reporting::Severity;
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

#[rstest]
#[case::empty_directory(false)]
#[case::disabled_lints(true)]
fn run_should_reject_explicit_baseline_without_snapshot_inputs(#[case] file: bool) {
    let directory = tempfile::tempdir().unwrap();
    if file {
        fs::write(directory.path().join("input.rs"), "fn f() {}").unwrap();
    }
    let options = RunOptions {
        paths: vec![directory.path().into()],
        include: vec!["links".into()],
        diff_base: Some("missing-reference".into()),
        ..RunOptions::default()
    };

    let error = run(&options, None).unwrap_err();

    assert!(format!("{error:#}").contains("explicit baseline"));
}

#[rstest]
#[case::warning("warning", Severity::Warning)]
#[case::hint("hint", Severity::Hint)]
#[case::error("error", Severity::Error)]
fn run_should_report_configured_symbol_severity_without_git(
    #[case] configured: &str,
    #[case] expected: Severity,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join(".rust-llm-tidy.yml");
    fs::write(&path, "fn f() { run(); }").unwrap();
    fs::write(
        &config_path,
        format!("symbol_rules: [{{symbol: run, message: finding, severity: {configured}}}]"),
    )
    .unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["SYM001".into()],
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    assert!(report.warnings.is_empty());
    assert_eq!(report.files[0].diagnostics.len(), 1);
    assert_eq!(report.files[0].diagnostics[0].severity, expected);
    assert_eq!(
        report.ensure_success().is_err(),
        expected == Severity::Error
    );
}

#[rstest]
#[case::default(false, None, 0)]
#[case::git_without_repository(true, None, 0)]
#[case::audit(false, Some(ReportingScope::All), 1)]
fn run_should_skip_reminders_without_a_baseline_unless_auditing(
    #[case] git_changed: bool,
    #[case] scope: Option<ReportingScope>,
    #[case] count: usize,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    fs::write(&path, "fn f() { Vec::new(); }").unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["PERF001".into()],
        git_changed,
        lint_scope: scope,
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert_eq!(report.files[0].diagnostics.len(), count);
    assert_eq!(
        report.warnings.is_empty(),
        scope == Some(ReportingScope::All)
    );
    report.ensure_success().unwrap();
}

#[rstest]
#[case::default(None, 0)]
#[case::changed(Some(ReportingScope::ChangedLines), 0)]
#[case::audit(Some(ReportingScope::All), 1)]
fn source_should_apply_reminder_scope_without_git(
    #[case] scope: Option<ReportingScope>,
    #[case] count: usize,
) {
    let options = SourceOptions {
        include: vec!["PERF001".into()],
        lint_scope: scope,
        ..SourceOptions::default()
    };

    let report = tidy_source("fn f() { Vec::new(); }", "rs", &options).unwrap();

    assert_eq!(report.diagnostics.len(), count);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == Severity::Reminder)
    );
}
