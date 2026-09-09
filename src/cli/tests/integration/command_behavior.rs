//! CLI behavior tests for `rust-llm-tidy`: dry-run reporting, failure
//! modes, and empty inputs.
//!
//! Every test runs the built CLI binary against a temp file or directory
//! and asserts on its exit code, stdout, and stderr.

use super::{binary, run, run_command, run_dir, temp_dir};
use rstest::rstest;
use std::fs;
use std::process::{Command, Output};

/// Findings retain their severity-based exit status in a non-mutating preview.
#[rstest]
#[case::warning("warning", 0)]
#[case::hint("hint", 0)]
#[case::reminder("reminder", 0)]
#[case::error("error", 1)]
fn dry_run_should_fail_only_for_error_findings(
    #[case] severity: &str,
    #[values("text", "json")] mode: &str,
    #[case] exit: i32,
) {
    let source = "fn f() { needle(); }\n";
    let config = format!(
        "perf_hints: []\nsymbol_rules: [{{regex: needle, title: Review, message: finding, severity: {severity}}}]"
    );

    let (output, consumed) = preview(source, "SYM", &config, mode);

    assert_eq!(output.status.code(), Some(exit), "{output:?}");
    assert_eq!(consumed, source.as_bytes());
    if mode == "json" {
        let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["severity"], severity);
        assert_eq!(records[0]["code"], "SYM");
    } else {
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(&format!("{severity}[SYM]")), "{stderr}");
    }
}

// ── CLI behavior tests ────────────────────────────────────────────

#[test]
fn dry_run_should_preserve_lint_failure_when_edits_are_needed() {
    let source = "pub fn a() {}\npub fn b() { a(); }\n";

    let (output, consumed) = preview(source, "reorder,DOC001", "{}", "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(consumed, source.as_bytes());
    assert!(String::from_utf8_lossy(&output.stderr).contains("found 2 error(s)"));
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(records.iter().any(|record| record["severity"] == "success"));
}

/// Preview reports needed edits or processing failures without writing source.
#[rstest]
#[case::clean("fn a() {}\n", "reorder", 0, None, None)]
#[case::empty("", "reorder", 0, None, None)]
#[case::reorder(
    "fn a() {}\nfn b() { a(); }\n",
    "reorder",
    1,
    Some("REORDER"),
    Some("dry-run found proposed transformations")
)]
#[case::crlf_reorder(
    "fn a() {}\r\nfn b() { a(); }\r\n",
    "reorder",
    1,
    Some("REORDER"),
    Some("dry-run found proposed transformations")
)]
#[case::visibility(
    "pub(crate) mod m { pub fn f() {} }\n",
    "vis",
    1,
    Some("VIS"),
    Some("dry-run found proposed transformations")
)]
#[case::links(
    "/// See [A](https://example.invalid).\npub struct A;\n",
    "links",
    1,
    Some("FIX"),
    Some("dry-run found proposed transformations")
)]
#[case::parse_failure(
    "not valid rust {{{",
    "reorder",
    1,
    None,
    Some("failed to process 1 file(s)")
)]
fn dry_run_should_report_status_without_writing_source(
    #[case] source: &str,
    #[case] rule: &str,
    #[values("text", "json")] mode: &str,
    #[case] exit: i32,
    #[case] change_code: Option<&str>,
    #[case] failure: Option<&str>,
) {
    let (output, consumed) = preview(source, rule, "{}", mode);

    assert_eq!(output.status.code(), Some(exit), "{output:?}");
    assert_eq!(consumed, source.as_bytes());
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(failure) = failure {
        assert!(stderr.contains(failure), "{stderr}");
    }
    if mode == "json" {
        let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(records.len(), usize::from(change_code.is_some()));
        if let Some(code) = change_code {
            assert_eq!(records[0]["code"], code);
            assert_eq!(records[0]["severity"], "success");
        }
    } else {
        assert!(output.stdout.is_empty());
        if let Some(code) = change_code {
            assert!(stderr.contains(&format!("success[{code}]")), "{stderr}");
            assert!(!stderr.contains(source), "must not echo source: {stderr}");
        }
    }
}

/// An empty directory is accepted and produces no output.
#[test]
fn empty_directory_should_run_cleanly() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let (stdout, stderr, exit) = run_dir(&dir, &[]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit, 0, "empty directory should exit successfully");
    assert!(
        stdout.is_empty(),
        "stdout should be empty for empty directory"
    );
    assert!(stderr.is_empty(), "stderr should be empty on success");
}

/// Safety check: parse-invalid source must cause an error exit.
/// We verify that rust-llm-tidy exits non-zero when given a non-Rust file.
#[test]
fn invalid_source_should_abort_with_error() {
    let source = "not valid rust {{{";

    let (_stdout, stderr, exit) = run(source, &[]);

    assert_ne!(exit, 0, "rust-llm-tidy should exit non-zero on parse error");
    assert!(!stderr.is_empty(), "stderr should contain error message");
}

/// A non-existent path is rejected with an error exit.
#[test]
fn nonexistent_path_should_fail_with_error() {
    let nonexistent = std::env::temp_dir().join(format!(
        "rust-llm-tidy-missing-{}-{}-{}-{}-{}-{}-{}-{}-{}.rs",
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id(),
        std::process::id()
    ));

    let output = run_command(&["--include", "reorder"], &nonexistent);
    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).is_empty(),
        "stderr should report the missing path"
    );
}

/// Run a preview with explicit config and no inherited baseline, retaining bytes.
fn preview(source: &str, rule: &str, config: &str, mode: &str) -> (Output, Vec<u8>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    fs::write(&path, source).unwrap();
    fs::write(directory.path().join("config.yml"), config).unwrap();

    let output = Command::new(binary())
        .current_dir(directory.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args([
            "--config",
            "config.yml",
            "--all-lines",
            "--dry-run",
            "--output-mode",
            mode,
            "input.rs",
        ])
        .args(rule.split(',').flat_map(|rule| ["--include", rule]))
        .output()
        .unwrap();

    (output, fs::read(path).unwrap())
}
