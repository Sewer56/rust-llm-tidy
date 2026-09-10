//! Corpus-gate tests for the `rust-llm-tidy` CLI.
//!
//! Every `_after` fixture must be idempotent on rerun, and an in-place
//! write must reproduce an `_after` fixture. A dry-run over this
//! repository, with its own config active, must emit zero change records.

use super::{TEST_COUNTER, binary, manifest_dir, run_command, run_dry_run};
use core::sync::atomic::Ordering;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};

// ── Idempotency: every _after fixture must be unchanged ───────────

/// Idempotency: every `_after.*` fixture, in every language directory
/// under `tests/fixtures/reorder/`, must be unchanged by a second run.
#[test]
fn all_after_fixtures_should_be_idempotent_on_rerun() {
    let fixture_root = manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder");
    // Each language directory under the fixture root holds its own
    // `<name>_after.<ext>` fixture pairs.
    let mut after_files: Vec<PathBuf> = Vec::new();
    for lang in fs::read_dir(&fixture_root).unwrap() {
        let lang_dir = lang.unwrap().path().read_dir().unwrap();
        for entry in lang_dir {
            let path = entry.unwrap().path();
            let is_after = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| name.contains("_after."));
            if is_after {
                after_files.push(path);
            }
        }
    }
    after_files.sort();

    assert!(!after_files.is_empty(), "no _after fixtures found");

    for after_path in &after_files {
        let (stdout, stderr, exit) = run_dry_run(after_path);

        assert_eq!(exit, 0, "{} dry-run should succeed", after_path.display());
        assert!(
            stdout.is_empty(),
            "{} dry-run must not print source to stdout",
            after_path.display()
        );
        assert!(
            stderr.is_empty(),
            "{} is already tidy: dry-run must emit zero change records",
            after_path.display()
        );
    }
}

/// In-place write: copy a synthetic before fixture to a temp file, run without
/// `--dry-run`, and verify the file content matches the after fixture.
#[test]
fn in_place_write_should_match_after_fixture() {
    let expected = include_str!("../fixtures/reorder/rust/phase_use_stable_after.rs");

    let dir = env::temp_dir();
    let pid = process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("rust-llm-tidy-write-test-{}-{}.rs", pid, seq));

    fs::write(
        &tmp,
        include_str!("../fixtures/reorder/rust/phase_use_stable_before.rs"),
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "rust-llm-tidy (no --dry-run) failed"
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);

    assert_eq!(
        actual, expected,
        "in-place write: temp file content must match phase_use_stable_after.rs"
    );
}

// ── Corpus gate ────────────────────────────────────────────────────

/// The repository corpus gate checks that every tracked file is already tidy.
///
/// - A `--dry-run` over this repository's root exits 0 and emits zero
///   change records.
/// - The repo config is active.
/// - TEST002 stays out of this gate while the repository's test functions
///   still lack summaries.
#[test]
fn repo_corpus_dry_run_emits_zero_change_records() {
    let root = manifest_dir()
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve");
    // Guard against a vacuous pass: only this repository's root holds both
    // the workspace manifest and the tidy config, so the walk below covers
    // real files.
    assert!(root.join("Cargo.toml").is_file());
    assert!(root.join(".rust-llm-tidy.yml").is_file());

    let output = Command::new(binary())
        .current_dir(&root)
        .args(["--dry-run", "--exclude", "TEST002", "."])
        .output()
        .expect("failed to spawn rust-llm-tidy over the repo root");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "corpus dry-run must exit 0: {stderr}"
    );
    assert!(stdout.is_empty(), "dry-run prints nothing to stdout");
    assert!(
        !stderr.contains("success["),
        "the whole repository must emit zero change records: {stderr}"
    );
}
