//! TEXT007 default enablement, opt-out, selection, and changed-line reporting.

use super::common::binary;
use rstest::rstest;
use std::fs;
use std::process::Command;

/// A controlled baseline separates unchanged prose from the edited line.
#[rstest]
#[case::default(None, &[], &[3])]
#[case::empty_config(Some("{}"), &[], &[3])]
#[case::explicit_code(None, &["--include", "TEXT007"], &[3])]
#[case::all_lines(None, &["--all-lines"], &[1, 3])]
#[case::configured_all(Some("lint_scopes: {TEXT007: all}"), &[], &[1, 3])]
#[case::disabled(Some("passive_narration: {enable: false}"), &[], &[])]
#[case::disabled_audit(Some("passive_narration: {enable: false}"), &["--all-lines"], &[])]
fn text007_should_report_eligible_lines_against_git_baseline(
    #[case] yaml: Option<&str>,
    #[case] args: &[&str],
    #[case] expected: &[u64],
) {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    let file = dir.path().join("notes.md");
    fs::write(
        &file,
        "Errors are returned by the scanner.\n\nRead the input.\n",
    )
    .unwrap();
    git(&["add", "notes.md"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.test",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "--quiet",
        "-m",
        "baseline",
    ]);
    if let Some(yaml) = yaml {
        fs::write(dir.path().join(".rust-llm-tidy.yml"), yaml).unwrap();
    }
    fs::write(
        &file,
        "Errors are returned by the scanner.\n\nInput is parsed by the scanner.\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .current_dir(dir.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args(["--diff-base", "HEAD", "--json"])
        .args(args)
        .arg(&file)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let findings: Vec<_> = records
        .iter()
        .filter(|record| record["code"] == "TEXT007")
        .collect();
    let lines: Vec<_> = findings
        .iter()
        .map(|record| record["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, expected);
    assert!(
        findings
            .iter()
            .all(|record| record["severity"] == "reminder")
    );
}

/// Explicit code selection overrides the config opt-out, but not exclusions.
#[rstest]
#[case::default("{}", &[], true)]
#[case::group("{}", &["--include", "lints"], true)]
#[case::suppression_only("passive_narration: {suppress_in_release_notes: false}", &[], true)]
#[case::enabled("passive_narration: {enable: true}", &[], true)]
#[case::disabled("passive_narration: {enable: false}", &[], false)]
#[case::disabled_group("passive_narration: {enable: false}", &["--include", "lints"], false)]
#[case::explicit_code("passive_narration: {enable: false}", &["--include", "TEXT007"], true)]
#[case::explicit_group_and_code(
    "passive_narration: {enable: false}", &["--include", "lints", "--include", "TEXT007"], true
)]
#[case::config_code("passive_narration: {enable: false}\ninclude: [{rules: [TEXT007]}]", &[], true)]
#[case::config_group("include: [{rules: [lints]}]", &[], true)]
#[case::other_selection("{}", &["--include", "TEXT006"], false)]
#[case::excluded("{}", &["--exclude", "TEXT007"], false)]
fn text007_should_respect_enablement_and_selection(
    #[case] yaml: &str,
    #[case] args: &[&str],
    #[case] expected: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".rust-llm-tidy.yml");
    fs::write(&cfg, yaml).unwrap();
    let file = dir.path().join("notes.md");
    fs::write(&file, "Errors are returned by the scanner.\n").unwrap();

    let output = Command::new(binary())
        .args(["--all-lines", "--config"])
        .arg(&cfg)
        .args(args)
        .arg(&file)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("reminder[TEXT007]"), expected, "{stderr}");
}
