//! TEXT010 documentation-context detection through file and buffer entry points.

use rstest::rstest;
use rust_llm_tidy::reporting::{Diagnostic, Severity};
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

/// Markdown used by most cases: a blank first line, then prose.
const BLANK_OPENER: &str = "\n# Guide\n\nBody text.\n";

// ── Context-free entry points ──

/// Buffers have no file context: TEXT010 never fires and never reads files.
#[test]
fn buffer_audits_should_never_emit_the_reminder() {
    let options = SourceOptions {
        include: vec!["TEXT010".into()],
        all_lines: true,
        ..SourceOptions::default()
    };

    let report = tidy_source(BLANK_OPENER, "md", &options).unwrap();

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(report.source.as_ref(), BLANK_OPENER);
}

// ── Detection signals ──

#[rstest]
#[case::readme(&[("README.MD", BLANK_OPENER)], "README.MD", "README filename")]
#[case::quickstart(&[("QUICKSTART.md", BLANK_OPENER)], "QUICKSTART.md", "QUICKSTART filename")]
#[case::getting_started(
    &[("GETTING_STARTED.mdx", BLANK_OPENER)],
    "GETTING_STARTED.mdx",
    "GETTING_STARTED filename"
)]
#[case::docs(&[("docs/setup.md", BLANK_OPENER)], "docs/setup.md", "file is under docs/")]
#[case::docs_case(
    &[("Docs/Setup.MARKDOWN", BLANK_OPENER)],
    "Docs/Setup.MARKDOWN",
    "file is under docs/"
)]
#[case::mkdocs(
    &[("mkdocs.yml", "site_name: docs\n"), ("guide.md", BLANK_OPENER)],
    "guide.md",
    "nearby mkdocs.yml"
)]
#[case::mdbook(
    &[("book.toml", "[book]\n"), ("guide.md", BLANK_OPENER)],
    "guide.md",
    "nearby book.toml"
)]
#[case::readthedocs(
    &[(".readthedocs.yaml", "version: 2\n"), ("guide.md", BLANK_OPENER)],
    "guide.md",
    "nearby .readthedocs.yaml"
)]
fn detected_files_should_receive_one_reminder(
    #[case] files: &[(&str, &str)],
    #[case] target: &str,
    #[case] reason: &str,
) {
    let diagnostics = audit(files, target);

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "TEXT010");
    assert_eq!(diagnostics[0].severity, Severity::Reminder);
    assert_eq!(diagnostics[0].item_kind, "file");
    assert_eq!(diagnostics[0].title(), "documentation audience review");
    // Detection skips the blank opening line: the reminder anchors to line 2.
    assert_eq!(diagnostics[0].line, 2);
    assert!(
        diagnostics[0].message.contains(reason),
        "{:?}",
        diagnostics[0]
    );
}

/// Exclusion suppresses the reminder even in an all-lines audit.
#[test]
fn exclusion_should_suppress_the_reminder() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("docs/setup.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, BLANK_OPENER).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["TEXT010".into()],
        exclude: vec!["TEXT010".into()],
        all_lines: true,
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert!(report.files[0].diagnostics.is_empty());
}

// ── Message and scope behavior ──

/// The reminder carries the shared guidance, including the audience branch.
#[test]
fn reminder_should_carry_the_documentation_guidance() {
    let diagnostics = audit(&[("docs/setup.md", BLANK_OPENER)], "docs/setup.md");
    let message = &diagnostics[0].message;

    for expected in [
        "If this is end-user documentation, explain usage and relevant outcomes.",
        "the shortest useful getting-started path",
        "use a brief admonition near the section start",
        "leave suitable documentation unchanged",
    ] {
        assert!(
            message.contains(expected),
            "missing {expected} in {message:?}"
        );
    }
}

/// Changed-line scope without Git permission hides the reminder and warns.
#[test]
fn reminder_should_stay_hidden_without_changed_lines() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("docs/setup.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, BLANK_OPENER).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["TEXT010".into()],
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert!(report.files[0].diagnostics.is_empty());
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
}

#[rstest]
#[case::agents(&[("docs/AGENTS.md", BLANK_OPENER)], "docs/AGENTS.md")]
#[case::agent_case(&[("AGENTS.MD", BLANK_OPENER)], "AGENTS.MD")]
#[case::non_prose(&[("docs/example.rs", "fn f() {}\n")], "docs/example.rs")]
#[case::similar_directory(&[("mydocs/notes.md", BLANK_OPENER)], "mydocs/notes.md")]
#[case::similar_file(&[("docs.md", BLANK_OPENER)], "docs.md")]
fn undetected_files_should_stay_silent(#[case] files: &[(&str, &str)], #[case] target: &str) {
    assert!(audit(files, target).is_empty());
}

/// Write `files` under one temp root and audit `target` for TEXT010.
fn audit(files: &[(&str, &str)], target: &str) -> Vec<Diagnostic> {
    let directory = tempfile::tempdir().unwrap();
    for (relative, content) in files {
        let path = directory.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }
    let options = RunOptions {
        paths: vec![directory.path().join(target)],
        include: vec!["TEXT010".into()],
        all_lines: true,
        ..RunOptions::default()
    };

    run(&options, None).unwrap().files.remove(0).diagnostics
}
