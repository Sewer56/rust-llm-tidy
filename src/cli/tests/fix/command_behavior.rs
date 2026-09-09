//! `fix` exit codes, change records, and in-place writes.
//!
//! Dry runs leave stdout empty and report one record per file on stderr;
//! in-place runs report the same records and write the file. The shared
//! runner helpers live in `mod.rs`.

use super::{TEST_COUNTER, fixture_dir, run_command, temp_dir, temp_file};
use core::sync::atomic::Ordering;
use std::fs;

/// `fix --dry-run` on `table_doc_comment_before.rs` reports a change record on
/// stderr and leaves stdout empty.
#[test]
fn fix_doc_comment_dry_run_reports_change() {
    let before = fixture_dir().join("table_doc_comment_before.rs");
    let output = run_command(&["--include", "tables", "--dry-run"], &before);

    assert!(
        !output.status.success(),
        "fix --dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        "dry-run must report a fix change line on stderr: {stderr}"
    );
}

/// In-place `fix --include fences` on `fence_md_before.md` produces
/// `fence_md_after.md` byte-for-byte (the fences transform's content output).
#[test]
fn fix_fence_in_place_matches_after() {
    let expected = fs::read_to_string(fixture_dir().join("fence_md_after.md")).unwrap();
    let tmp = temp_file("md");
    fs::write(
        &tmp,
        fs::read_to_string(fixture_dir().join("fence_md_before.md")).unwrap(),
    )
    .unwrap();

    let output = run_command(&["--include", "fences"], &tmp);
    assert!(
        output.status.success(),
        "fence fix in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(actual, expected, "fence fix must match fence_md_after.md");
}

/// Idempotency: running `fix --dry-run` on an `_after` fixture is a no-op with
/// zero change records.
#[test]
fn fix_idempotent_on_after_fixtures() {
    for name in ["table_md_after.md", "table_doc_comment_after.rs"] {
        let path = fixture_dir().join(name);
        let output = run_command(&["--include", "tables", "--dry-run"], &path);
        assert!(
            output.status.success(),
            "fix --dry-run on {name} should succeed"
        );
        assert!(
            output.stdout.is_empty(),
            "{name} dry-run must not print source to stdout"
        );
        assert!(
            output.stderr.is_empty(),
            "{name} is already tidy: dry-run must emit zero change records"
        );
    }
    // Fence fixture uses --include fences.
    {
        let path = fixture_dir().join("fence_md_after.md");
        let output = run_command(&["--include", "fences", "--dry-run"], &path);
        assert!(
            output.status.success(),
            "fix --dry-run on fence_md_after.md should succeed"
        );
        assert!(
            output.stdout.is_empty(),
            "fence_md_after.md dry-run must not print source to stdout"
        );
        assert!(
            output.stderr.is_empty(),
            "fence_md_after.md is already tidy: dry-run must emit zero change records"
        );
    }
}

/// An in-place fix run reports the same change records as its dry-run twin
/// and writes the file. Identical stderr change lines therefore accompany a
/// modified file.
#[test]
fn fix_in_place_reports_same_records_and_writes() {
    let before = fixture_dir().join("multi_md_before.md");
    let dry_run = run_command(&["--include", "tables", "--dry-run"], &before);
    let dry_stderr = String::from_utf8_lossy(&dry_run.stderr);

    let tmp = temp_file("md");
    fs::write(&tmp, fs::read_to_string(&before).unwrap()).unwrap();
    let output = run_command(&["--include", "tables"], &tmp);
    assert!(
        output.status.success(),
        "fix in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("success[FIX]").count(),
        1,
        "in-place run reports the same record: {stderr}"
    );
    assert!(
        stderr.contains("tables were aligned"),
        "in-place change line matches dry-run: {stderr}"
    );
    assert_eq!(
        stderr.matches("success[FIX]").count(),
        dry_stderr.matches("success[FIX]").count(),
        "in-place reports the same change lines as its dry-run twin"
    );
    assert_ne!(
        actual,
        fs::read_to_string(&before).unwrap(),
        "in-place run must write the fixed file"
    );
}

/// In-place write: copy before.md to a temp file, run `fix`, assert content.
#[test]
fn fix_in_place_write() {
    let expected = fs::read_to_string(fixture_dir().join("table_md_after.md")).unwrap();
    let tmp = temp_file("md");
    fs::write(
        &tmp,
        fs::read_to_string(fixture_dir().join("table_md_before.md")).unwrap(),
    )
    .unwrap();

    let output = run_command(&["--include", "tables"], &tmp);
    assert!(
        output.status.success(),
        "fix in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(actual, expected, "in-place file must match _after fixture");
}

/// `fix --dry-run` on `table_md_before.md` reports a change record on stderr
/// and leaves stdout empty.
#[test]
fn fix_md_dry_run_reports_change() {
    let before = fixture_dir().join("table_md_before.md");
    let output = run_command(&["--include", "tables", "--dry-run"], &before);

    assert!(
        !output.status.success(),
        "fix --dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        "dry-run must report a fix change line on stderr: {stderr}"
    );
}

/// `fix --include tables --dry-run` on a fixture with two misaligned tables
/// reports one per-file record, not one per table.
#[test]
fn fix_multi_entity_dry_run_reports_one_record_per_file() {
    let before = fixture_dir().join("multi_md_before.md");
    let output = run_command(&["--include", "tables", "--dry-run"], &before);

    assert!(
        !output.status.success(),
        "fix --dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("success[FIX]").count(),
        1,
        "one record for the whole file: {stderr}"
    );
    assert!(
        stderr.contains("tables were aligned"),
        "record covers both tables with no line: {stderr}"
    );
}

/// A non-existent path is rejected.
#[test]
fn fix_nonexistent_path_fails() {
    let nonexistent = std::env::temp_dir().join(format!(
        "rust-llm-tidy-fix-missing-{}-{}.md",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let output = run_command(&["--include", "tables"], &nonexistent);
    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
}

/// Recursive directory: `fix` collects both `.rs` and `.md` files.
#[test]
fn fix_recursive_directory_collects_md_and_rs() {
    let dir = temp_dir();
    let sub = dir.join("sub");
    fs::create_dir_all(&sub).unwrap();

    fs::write(
        dir.join("readme.md"),
        fs::read_to_string(fixture_dir().join("table_md_before.md")).unwrap(),
    )
    .unwrap();
    fs::write(
        sub.join("code.rs"),
        fs::read_to_string(fixture_dir().join("table_doc_comment_before.rs")).unwrap(),
    )
    .unwrap();

    let output = run_command(&["--include", "tables"], &dir);
    assert!(
        output.status.success(),
        "fix directory should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let md_expected = fs::read_to_string(fixture_dir().join("table_md_after.md")).unwrap();
    let rs_expected = fs::read_to_string(fixture_dir().join("table_doc_comment_after.rs")).unwrap();

    assert_eq!(
        fs::read_to_string(dir.join("readme.md")).unwrap(),
        md_expected,
        ".md file should be fixed"
    );
    assert_eq!(
        fs::read_to_string(sub.join("code.rs")).unwrap(),
        rs_expected,
        ".rs file should be fixed"
    );

    let _ = fs::remove_dir_all(&dir);
}
