//! Integration tests for the `fix` subcommand of `rust-llm-tidy`.
//!
//! Mirrors the helper pattern from `doc_check.rs` (`run_command`, `binary`,
//! `manifest_dir`, `fixture_dir`). Each test runs the built CLI binary against
//! fixture files in `tests/fixtures/fix/`.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `fix --dry-run` on `table_doc_comment_before.rs` matches `_after.rs`.
#[test]
fn fix_doc_comment_dry_run_matches_after() {
    let before = fixture_dir().join("table_doc_comment_before.rs");
    let expected = fs::read_to_string(fixture_dir().join("table_doc_comment_after.rs")).unwrap();
    let output = run_command(&["fix", "--dry-run"], &before);

    assert!(
        output.status.success(),
        "fix --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, expected,
        "dry-run stdout must match table_doc_comment_after.rs"
    );
}

/// `fix --dry-run` on `fence_md_before.md` matches `fence_md_after.md`.
#[test]
fn fix_fence_md_dry_run_matches_after() {
    let before = fixture_dir().join("fence_md_before.md");
    let expected = fs::read_to_string(fixture_dir().join("fence_md_after.md")).unwrap();
    let output = run_command(&["fix", "--dry-run"], &before);

    assert!(
        output.status.success(),
        "fix --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, expected,
        "dry-run stdout must match fence_md_after.md"
    );
}

/// Idempotency: running `fix --dry-run` on an `_after` fixture is a no-op.
#[test]
fn fix_idempotent_on_after_fixtures() {
    for name in [
        "table_md_after.md",
        "fence_md_after.md",
        "table_doc_comment_after.rs",
    ] {
        let path = fixture_dir().join(name);
        let expected = fs::read_to_string(&path).unwrap();
        let output = run_command(&["fix", "--dry-run"], &path);
        assert!(
            output.status.success(),
            "fix --dry-run on {name} should succeed"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            &*stdout, &*expected,
            "{name} must be idempotent (output unchanged)"
        );
    }
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

    let output = run_command(&["fix"], &tmp);
    assert!(
        output.status.success(),
        "fix in-place should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(actual, expected, "in-place file must match _after fixture");
}

/// `fix --dry-run` on `table_md_before.md` matches `table_md_after.md`.
#[test]
fn fix_md_dry_run_matches_after() {
    let before = fixture_dir().join("table_md_before.md");
    let expected = fs::read_to_string(fixture_dir().join("table_md_after.md")).unwrap();
    let output = run_command(&["fix", "--dry-run"], &before);

    assert!(
        output.status.success(),
        "fix --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, expected,
        "dry-run stdout must match table_md_after.md"
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
    let output = run_command(&["fix"], &nonexistent);
    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
}

// -- Helpers (mirrors doc_check.rs) -----------------------------------

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

    let output = run_command(&["fix"], &dir);
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

/// The directory holding `fix` fixtures.
fn fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("fix")
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-fix-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary file path.
fn temp_file(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-fix-{}-{}.{}", pid, seq, ext))
}

/// Return the path to the `rust-llm-tidy` debug binary.
fn binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let dir = std::env::current_dir().unwrap();
            dir.join("../target/debug/rust-llm-tidy")
        })
}

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
