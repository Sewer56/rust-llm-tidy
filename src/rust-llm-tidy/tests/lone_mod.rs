//! MOD005 selection and read-only behavior through the public file pipeline.

use rstest::rstest;
use rust_llm_tidy::reporting::Severity;
use rust_llm_tidy::{RunOptions, config, run};
use std::fs;

/// Rule selection applies through both CLI options and compiled configuration.
#[rstest]
#[case::default(&[], &[], None, true)]
#[case::included(&["MOD005"], &[], None, true)]
#[case::lint_group(&["lints"], &[], None, true)]
#[case::other_rule(&["MOD002"], &[], None, false)]
#[case::excluded(&[], &["MOD005"], None, false)]
#[case::group_excluded(&[], &["lints"], None, false)]
#[case::config_included(&[], &[], Some("include: [{rules: [MOD005]}]"), true)]
#[case::config_excluded(&[], &[], Some("exclude: [{rules: [MOD005]}]"), false)]
#[case::file_excluded(&[], &[], Some("exclude_files: ['banana/mod.rs']"), false)]
fn run_should_honor_selection_when_directory_contains_only_mod(
    #[case] include: &[&str],
    #[case] exclude: &[&str],
    #[case] config_yaml: Option<&str>,
    #[case] expected: bool,
) {
    // Arrange.
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("banana");
    fs::create_dir(&directory).unwrap();
    let path = directory.join("mod.rs");
    let source = "//! Banana.\nfn peel() {}\n";
    fs::write(&path, source).unwrap();
    let compiled = config_yaml.map(|yaml| {
        let config_path = root.path().join("config.yml");
        fs::write(&config_path, yaml).unwrap();
        config::load_and_compile(&config_path).unwrap()
    });
    let options = RunOptions {
        paths: vec![path.clone()],
        include: include.iter().map(|rule| (*rule).into()).collect(),
        exclude: exclude.iter().map(|rule| (*rule).into()).collect(),
        ..RunOptions::default()
    };

    // Act.
    let report = run(&options, compiled.as_ref()).unwrap();

    // Assert.
    let diagnostics: Vec<_> = report
        .files
        .iter()
        .flat_map(|file| &file.diagnostics)
        .collect();
    let findings: Vec<_> = diagnostics.iter().filter(|d| d.code == "MOD005").collect();
    assert_eq!(findings.len(), usize::from(expected));
    if include == ["MOD005"] {
        assert_eq!(diagnostics.len(), 1);
    }
    if let Some(finding) = findings.first() {
        assert_eq!(finding.severity, Severity::Hint);
        assert_eq!(finding.line, 1);
        assert_eq!(
            finding.message,
            "directory `banana/` contains only `mod.rs`.\n\n\
             Why: This directory adds a navigation step without grouping other files.\n\n\
             Suggestions:\n\
             - Consider moving `banana/mod.rs` to `banana.rs` and removing the empty directory.\n\
             - Check relative `include!`, `include_str!`, `include_bytes!`, and `#[path]` paths,\n  \
               and explicit references to the old location. Preserve behavior; do not overwrite an existing file."
        );
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert!(!root.path().join("banana.rs").exists());
    assert!(report.files.iter().all(|file| file.changes.is_empty()));
}
