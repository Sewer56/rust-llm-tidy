//! `all` subcommand fix behavior and lint dispatch, not one lint family.
//!
//! Every test runs the built CLI binary on a markdown fixture or temp
//! file and asserts on its exit code, output, or written file.

use crate::{fix_fixture_dir, oversized_paragraph_md, run_command, temp_file, temp_md};
use std::fs;

/// `all --dry-run` previews table fixes to a `.md` file and reports the change
/// record on stderr, leaving stdout empty.
#[test]
fn all_md_dry_run_fixes_tables() {
    let before = fix_fixture_dir().join("table_md_before.md");
    let output = run_command(&["--dry-run"], &before);

    assert!(
        !output.status.success(),
        "all --dry-run on markdown must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        "all --dry-run must report the table fix on stderr: {stderr}"
    );
}

/// `all` fixes markdown tables in place but skips reorder/check for `.md`.
#[test]
fn all_md_in_place_fixes_tables() {
    let expected = fs::read_to_string(fix_fixture_dir().join("table_md_after.md")).unwrap();
    let tmp = temp_file("md");
    fs::write(
        &tmp,
        fs::read_to_string(fix_fixture_dir().join("table_md_before.md")).unwrap(),
    )
    .unwrap();

    let output = run_command(&[], &tmp);
    assert!(
        output.status.success(),
        "all on markdown file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(actual, expected, "in-place markdown fix must match after");
}

/// A clean markdown file passes lint dispatch with no diagnostics.
#[test]
fn md_clean_file_no_diagnostics() {
    let path = temp_md("# Title\n\nShort text paragraph.\n");
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "clean markdown should pass lint dispatch: {stderr}"
    );
    assert!(
        stderr.is_empty(),
        "clean markdown should produce no diagnostics, got:\n{stderr}"
    );
}

/// A whitelist without `lints` (or any lint code) skips linting entirely,
/// including the markdown text checks.
#[test]
fn md_lints_skipped_when_whitelist_omits_lints() {
    let path = temp_md(&oversized_paragraph_md());
    let output = run_command(&["--include", "tables", "--dry-run"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "tables-only whitelist must not lint markdown: {stderr}"
    );
    assert!(
        !stderr.contains("TEXT001") && !stderr.contains("TEXT002"),
        "lint findings must be suppressed without `lints` in the whitelist:\n{stderr}"
    );
}
