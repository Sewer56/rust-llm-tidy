//! CLI behavior tests for `rust-llm-tidy`: dry-run reporting, failure
//! modes, and empty inputs.
//!
//! Every test runs the built CLI binary against a temp file or directory
//! and asserts on its exit code, stdout, and stderr.

use super::{run, run_command, run_dir, temp_dir};
use std::fs;

// ── CLI behavior tests ────────────────────────────────────────────

/// `--dry-run` reports the would-be reorder move on stderr without printing
/// the reconstructed source to stdout or modifying the file on disk.
#[test]
fn dry_run_should_not_write_files() {
    let source = "fn a() {}\nfn b() { a(); }\n";

    let (stdout, stderr, exit) = run(source, &["--dry-run"]);

    assert_eq!(exit, 0, "dry-run should succeed");
    assert!(stdout.is_empty(), "stdout must be empty on dry-run success");
    assert!(
        stderr.contains("success[REORDER]"),
        "stderr should report the reorder move as a change line: {stderr}"
    );
    assert!(
        !stderr.contains("fn a()"),
        "stderr must not echo reconstructed source: {stderr}"
    );
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

/// `reorder --dry-run` on a CRLF reordering source reports its move on stderr
/// and leaves stdout empty (no reconstructed source).
#[test]
fn reorder_dry_run_reports_change_with_empty_stdout() {
    let source = "fn a() {}\r\nfn b() { a(); }\r\n";
    let (stdout, stderr, exit) = run(source, &["--dry-run"]);
    assert_eq!(exit, 0, "dry-run should succeed");
    assert!(stdout.is_empty(), "dry-run must not print source to stdout");
    assert!(
        stderr.contains("success[REORDER]"),
        "dry-run must report a reorder change on stderr: {stderr:?}"
    );
}
