//! File and buffer acceptance coverage for configurable text regex hints.

use rstest::rstest;
use rust_llm_tidy::config::{ReportingScope, load_and_compile};
use rust_llm_tidy::reporting::Severity;
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

#[rstest]
#[case::default(None, false, None)]
#[case::entry_all(Some(ReportingScope::All), false, Some(1))]
#[case::entry_changed(Some(ReportingScope::ChangedLines), false, Some(0))]
#[case::override_default(None, true, Some(1))]
#[case::override_entry(Some(ReportingScope::ChangedLines), true, Some(1))]
fn buffer_should_apply_text_hint_reporting_scope(
    #[case] scope: Option<ReportingScope>,
    #[values(Severity::Reminder, Severity::Hint, Severity::Warning, Severity::Error)]
    severity: Severity,
    #[case] all_lines: bool,
    #[case] count: Option<usize>,
) {
    let mut rule: rust_llm_tidy::config::SymbolRule =
        serde_yml::from_str("regex: needle\ntitle: Review\nmessage: finding").unwrap();
    rule.scope = scope;
    rule.severity = Some(severity);
    let options = SourceOptions {
        include: vec!["SYM".into()],
        text_rules: vec![rule],
        all_lines,
        ..SourceOptions::default()
    };

    let report = tidy_source("needle", "md", &options).unwrap();

    let count = count.unwrap_or(usize::from(severity != Severity::Reminder));
    assert_eq!(report.diagnostics.len(), count);
    if count != 0 {
        assert_eq!(report.diagnostics[0].severity, severity);
    }
}

#[rstest]
#[case::excluded_code("SYM", "SYM")]
#[case::excluded_lints("SYM", "lints")]
#[case::not_included("TEXT001", "")]
#[case::transform_only("links", "")]
fn buffer_should_not_enable_unselected_text_rules_when_all_lines_is_enabled(
    #[case] include: &str,
    #[case] exclude: &str,
) {
    let rule = serde_yml::from_str("regex: needle\ntitle: Review\nmessage: finding").unwrap();
    let options = SourceOptions {
        include: vec![include.into()],
        exclude: if exclude.is_empty() {
            Vec::new()
        } else {
            vec![exclude.into()]
        },
        text_rules: vec![rule],
        all_lines: true,
        ..SourceOptions::default()
    };

    let report = tidy_source("needle", "md", &options).unwrap();

    assert!(report.diagnostics.is_empty());
    assert!(report.warnings.is_empty());
    assert_eq!(report.source, "needle");
}

#[rstest]
#[case::symbol("symbol: needle")]
#[case::declaration("regex: needle\ntarget: declaration")]
fn buffer_should_reject_non_text_policies(#[case] matcher: &str) {
    let rule = serde_yml::from_str(&format!("{matcher}\ntitle: Review\nmessage: finding")).unwrap();
    let options = SourceOptions {
        text_rules: vec![rule],
        ..SourceOptions::default()
    };

    let error = tidy_source("needle", "md", &options).unwrap_err();

    assert!(error.to_string().contains("only usage regex hints"));
}

#[rstest]
#[case::markdown("needle\nneedle", "md", "include", 2, false)]
#[case::javascript("const s = 'needle'; // needle", "js", "exclude", 1, false)]
#[case::python("s = 'needle' # needle", "py", "only", 1, false)]
#[case::rust("fn f() { let s = \"needle\"; } // needle", "rs", "exclude", 1, false)]
#[case::invocation("fn f() { needle(); }", "rs", "include", 1, false)]
#[case::csharp(
    "class C { string s = \"needle\"; /* needle */ }",
    "cs",
    "only",
    1,
    false
)]
#[case::unknown_comments("needle", "txt", "only", 0, true)]
fn entrypoints_should_produce_equal_consumed_text_findings(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] comments: &str,
    #[case] count: usize,
    #[case] warning: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("input.{ext}"));
    let config_path = directory.path().join("config.yml");
    let rule =
        format!("regex: needle\ntitle: Review\nmessage: finding\ncomments: {comments}\nscope: all");
    fs::write(&path, source).unwrap();
    fs::write(
        &config_path,
        format!(
            "perf_hints: []\nsymbol_rules:\n  - {}",
            rule.replace('\n', "\n    ")
        ),
    )
    .unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["SYM".into()],
        ..RunOptions::default()
    };
    let source_options = SourceOptions {
        include: vec!["SYM".into()],
        text_rules: vec![serde_yml::from_str(&rule).unwrap()],
        ..SourceOptions::default()
    };

    let file = run(&options, Some(&config)).unwrap();
    let buffer = tidy_source(source, ext, &source_options).unwrap();

    assert_eq!(file.files[0].diagnostics, buffer.diagnostics);
    assert_eq!(buffer.diagnostics.len(), count);
    assert_eq!(!buffer.warnings.is_empty(), warning);
    assert_eq!(!file.warnings.is_empty(), warning);
    assert_eq!(buffer.source, source);
    file.ensure_success().unwrap();
}

#[test]
fn file_should_capture_changed_line_eligibility_for_text_regexes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.md");
    let config_path = directory.path().join("config.yml");
    fs::write(&path, "needle").unwrap();
    fs::write(
        &config_path,
        "symbol_rules: [{regex: needle, title: Review, message: finding}]",
    )
    .unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["SYM".into()],
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    assert!(report.files[0].diagnostics.is_empty());
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("Git reads were not granted"));
}
