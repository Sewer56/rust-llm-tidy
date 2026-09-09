//! Public reporting boundaries without ambient Git permission.

use rstest::rstest;
use rust_llm_tidy::config::load_and_compile;
use rust_llm_tidy::reporting::Severity;
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

#[test]
fn run_should_not_discover_inputs_when_only_all_lines_is_enabled() {
    let options = RunOptions {
        all_lines: true,
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert!(report.files.is_empty());
    assert!(report.warnings.is_empty());
}

#[rstest]
#[case::excluded_code("SYM", "SYM")]
#[case::excluded_lints("SYM", "lints")]
#[case::not_included("DOC001", "")]
#[case::transform_only("links", "")]
fn run_should_not_enable_unselected_lints_when_all_lines_is_enabled(
    #[case] include: &str,
    #[case] exclude: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    fs::write(&path, "fn f() { Vec::new(); }").unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec![include.into()],
        exclude: if exclude.is_empty() {
            Vec::new()
        } else {
            vec![exclude.into()]
        },
        all_lines: true,
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert!(report.files[0].diagnostics.is_empty());
    assert!(report.warnings.is_empty());
}

#[rstest]
#[case::error("pub fn load() {}", "DOC001", Severity::Error)]
#[case::warning("#[test]\nfn test_load() {}", "TEST001", Severity::Warning)]
#[case::hint("fn f() { std::mem::drop(0); }", "MOD003", Severity::Hint)]
fn run_should_override_configured_changed_line_scopes_for_builtin_lints(
    #[case] source: &str,
    #[case] code: &str,
    #[case] severity: Severity,
    #[values(false, true)] all_lines: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join("config.yml");
    fs::write(&path, source).unwrap();
    fs::write(
        &config_path,
        format!("lint_scopes: {{{code}: changed_lines}}"),
    )
    .unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec![code.into()],
        all_lines,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    assert_eq!(report.files[0].diagnostics.len(), usize::from(all_lines));
    if all_lines {
        assert_eq!(report.files[0].diagnostics[0].severity, severity);
    }
}

#[rstest]
#[case::default("", "", false, None)]
#[case::config_all("lint_scopes: {SYM: all}\n", "", false, Some(1))]
#[case::config_changed("lint_scopes: {SYM: changed_lines}\n", "", false, Some(0))]
#[case::entry_all("lint_scopes: {SYM: changed_lines}\n", ", scope: all", false, Some(1))]
#[case::entry_changed("lint_scopes: {SYM: all}\n", ", scope: changed_lines", false, Some(0))]
#[case::override_default("", "", true, Some(1))]
#[case::override_config("lint_scopes: {SYM: changed_lines}\n", "", true, Some(1))]
#[case::override_entry("", ", scope: changed_lines", true, Some(1))]
#[case::override_both(
    "lint_scopes: {SYM: changed_lines}\n",
    ", scope: changed_lines",
    true,
    Some(1)
)]
fn run_should_prioritize_all_lines_then_entry_then_config_then_severity(
    #[case] policy: &str,
    #[case] entry: &str,
    #[values("symbol", "regex")] matcher: &str,
    #[values(("warning", Severity::Warning), ("hint", Severity::Hint), ("error", Severity::Error), ("reminder", Severity::Reminder))]
    severity: (&str, Severity),
    #[case] all_lines: bool,
    #[case] count: Option<usize>,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join(".rust-llm-tidy.yml");
    fs::write(&path, "fn f() { run(); }").unwrap();
    fs::write(
        &config_path,
        format!("{policy}perf_hints: []\nsymbol_rules: [{{{matcher}: run, title: API, message: finding, severity: {}{entry}}}]", severity.0),
    )
    .unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["SYM".into()],
        all_lines,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    let count = count.unwrap_or(usize::from(severity.1 != Severity::Reminder));
    assert_eq!(report.warnings.is_empty(), count != 0);
    assert_eq!(report.files[0].diagnostics.len(), count);
    if count != 0 {
        assert_eq!(report.files[0].diagnostics[0].severity, severity.1);
    }
    assert_eq!(
        report.ensure_success().is_err(),
        count != 0 && severity.1 == Severity::Error
    );
}

#[rstest]
#[case::empty_directory(false)]
#[case::disabled_lints(true)]
fn run_should_reject_explicit_baseline_without_snapshot_inputs(
    #[case] file: bool,
    #[values(false, true)] all_lines: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    if file {
        fs::write(directory.path().join("input.rs"), "fn f() {}").unwrap();
    }
    let options = RunOptions {
        paths: vec![directory.path().into()],
        include: vec!["links".into()],
        diff_base: Some("missing-reference".into()),
        all_lines,
        ..RunOptions::default()
    };

    let error = run(&options, None).unwrap_err();

    assert!(format!("{error:#}").contains("explicit baseline"));
}

#[rstest]
#[case::default(false, false, 0)]
#[case::git_without_repository(true, false, 0)]
#[case::audit(false, true, 1)]
fn run_should_skip_reminders_without_a_baseline_unless_auditing(
    #[case] git_changed: bool,
    #[case] all_lines: bool,
    #[case] count: usize,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    fs::write(&path, "fn f() { Vec::new(); }").unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["SYM".into()],
        git_changed,
        all_lines,
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert_eq!(report.files[0].diagnostics.len(), count);
    assert_eq!(report.warnings.is_empty(), all_lines);
    report.ensure_success().unwrap();
}

#[rstest]
#[case::error("pub fn load() {}", "DOC001", Severity::Error)]
#[case::warning("#[test]\nfn test_load() {}", "TEST001", Severity::Warning)]
#[case::hint("fn f() { std::mem::drop(0); }", "MOD003", Severity::Hint)]
fn source_should_report_builtin_non_reminder_severities_on_all_lines(
    #[case] source: &str,
    #[case] code: &str,
    #[case] severity: Severity,
    #[values(false, true)] all_lines: bool,
) {
    let options = SourceOptions {
        include: vec![code.into()],
        all_lines,
        ..SourceOptions::default()
    };

    let report = tidy_source(source, "rs", &options).unwrap();

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].severity, severity);
}

#[rstest]
#[case::default(false, 0)]
#[case::audit(true, 1)]
fn source_should_report_reminders_without_git_when_all_lines_is_enabled(
    #[values("SYM", "TEXT007")] code: &str,
    #[case] all_lines: bool,
    #[case] count: usize,
) {
    let options = SourceOptions {
        include: vec![code.into()],
        all_lines,
        ..SourceOptions::default()
    };

    let report = tidy_source(
        "// Errors are returned by the scanner.\nfn f() { Vec::new(); }",
        "rs",
        &options,
    )
    .unwrap();

    assert_eq!(report.diagnostics.len(), count);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == Severity::Reminder)
    );
}
