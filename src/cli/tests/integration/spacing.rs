//! Check the CLI's member spacing in preview and apply modes.
//!
//! Copy fixtures to temporary files so apply-mode tests leave them alone.

use super::{manifest_dir, run_command, temp_file_ext};
use std::fs;
use std::path::PathBuf;

/// C# preview and apply produce the same spaced members.
#[test]
fn spacing_should_fix_csharp_members() {
    let before = fixture_copy("csharp_before.cs", "cs");
    let expected = fs::read_to_string(fixture_dir().join("csharp_after.cs")).unwrap();

    let output = run_command(&["--include", "spacing", "--dry-run"], &before);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert_eq!(
        stderr.matches("insert blank line between").count(),
        2,
        "{stderr}"
    );

    let output = run_command(&["--include", "spacing"], &before);
    assert!(output.status.success());
    assert_eq!(fs::read_to_string(&before).unwrap(), expected);

    let _ = fs::remove_file(&before);
}

/// Preview leaves the Rust fixture alone; apply matches the expected file.
#[test]
fn spacing_should_preview_and_then_write_the_after_fixture() {
    let before = fixture_copy("rust_before.rs", "rs");
    let expected = fs::read_to_string(fixture_dir().join("rust_after.rs")).unwrap();

    // Dry-run: three records, no write, non-zero exit for proposed
    // changes.
    let output = run_command(&["--include", "spacing", "--dry-run"], &before);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).is_empty());
    assert_eq!(
        stderr.matches("insert blank line between").count(),
        3,
        "one record per gap: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&before).unwrap(),
        fs::read_to_string(fixture_dir().join("rust_before.rs")).unwrap(),
        "dry-run must not write"
    );

    // In-place: the file matches the after fixture.
    let output = run_command(&["--include", "spacing"], &before);
    assert!(
        output.status.success(),
        "in-place spacing failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&before).unwrap(),
        expected,
        "in-place spacing must produce the after fixture"
    );

    // Idempotent: a dry-run over the spaced file reports nothing.
    let output = run_command(&["--include", "spacing", "--dry-run"], &before);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("success["), "{stderr}");

    let _ = fs::remove_file(&before);
}

/// Selecting FMT001 fails instead of silently enabling spacing.
#[test]
fn spacing_should_replace_the_retired_fmt001_code() {
    let path = fixture_copy("rust_before.rs", "rs");

    let output = run_command(&["--include", "FMT001"], &path);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("unknown op/rule `FMT001`"),
        "FMT001 must be rejected as unknown: {stderr}"
    );

    let _ = fs::remove_file(&path);
}

/// The default spaces members, while `--exclude spacing` leaves them packed.
#[test]
fn spacing_should_run_by_default_and_stay_excludable() {
    let excluded = fixture_copy("rust_before.rs", "rs");

    let output = run_command(&["--exclude", "spacing"], &excluded);
    assert!(output.status.success());
    // Normalize CRLF so the packed-members check holds on Windows checkouts.
    let after = fs::read_to_string(&excluded).unwrap().replace("\r\n", "\n");
    assert!(
        after.contains("pub source: u32,\n    ///"),
        "excluded spacing must leave members packed"
    );
    let _ = fs::remove_file(&excluded);

    let default = fixture_copy("rust_before.rs", "rs");
    let output = run_command(&[], &default);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("insert blank line between"),
        "default pipeline must space documented members: {stderr}"
    );
    // Normalize CRLF so the inserted-blank-line check holds on Windows checkouts.
    assert!(
        fs::read_to_string(&default)
            .unwrap()
            .replace("\r\n", "\n")
            .contains("pub source: u32,\n\n    ///"),
        "default pipeline must insert the blank line"
    );

    let _ = fs::remove_file(&default);
}

/// Checks-only neither edits nor reports spacing changes.
#[test]
fn spacing_should_stay_suppressed_under_checks_only() {
    let path = fixture_copy("rust_before.rs", "rs");

    let output = run_command(&["--checks-only"], &path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("success["), "{stderr}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        fs::read_to_string(fixture_dir().join("rust_before.rs")).unwrap()
    );

    let _ = fs::remove_file(&path);
}

/// Copy `name` into a numbered temp file with extension `ext`.
fn fixture_copy(name: &str, ext: &str) -> PathBuf {
    let path = temp_file_ext(ext);
    fs::copy(fixture_dir().join(name), &path).unwrap();
    path
}

/// The directory holding the spacing fixtures.
fn fixture_dir() -> PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("spacing")
}
