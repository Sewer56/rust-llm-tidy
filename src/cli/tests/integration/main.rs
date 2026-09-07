//! Integration tests for the `rust-llm-tidy` CLI.
//!
//! Tests are split into two groups:
//!
//! 1. Synthetic fixture tests (`tests/fixtures/reorder/<lang>/*_before.<ext>`
//!    → `*_after.<ext>`): one test per ordering/spacing rule, for `rust`
//!    and `csharp`.  Each fixture's header comment documents the rule and
//!    the expected before/after state.
//!
//! 2. CLI behavior tests: dry-run, in-place writes, directory traversal,
//!    error handling, and idempotency.
//!
//! Child modules:
//!
//! - `fixture_macros`: the `synthetic_fixture!`/`run_fixture!` macros.
//! - `rust_reorder`: Rust reorder fixtures and real-file reorder behavior.
//! - `csharp_reorder`: C# reorder fixtures and member-profile reorders.
//! - `command_behavior`: dry-run, failure, and empty-input CLI behavior.
//! - `directory_processing`: recursive directory collection and reporting.
//! - `language_selection`: extension flags and case-insensitive selection.
//! - `fences`: fence flipping and TEXT005 fence-lint warnings.
//! - `corpus`: repo-wide and fixture-corpus idempotency gates.
//!
//! The shared run/temp helpers live here; child modules import them with
//! `use super::...`.

use common::binary;
use core::sync::atomic::{AtomicU32, Ordering};
use std::fs;
use std::process::Command;

// Declared before the fixture modules: `#[macro_use]` puts the fixture
// macros in scope for every sibling module declared after it.
#[macro_use]
mod fixture_macros;
mod command_behavior;
mod corpus;
mod csharp_reorder;
mod directory_processing;
mod fences;
mod language_selection;
mod rust_reorder;
// The folder root sits inside `tests/integration/`, so the helpers shared by
// every test binary resolve at their sibling path, not under this folder.
#[path = "../common/mod.rs"]
mod common;

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

// ── Helpers ───────────────────────────────────────────────────────

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reorder a temp copy of `path` (keeping the given extension, which
/// selects the language backend) in place and return the rewritten content.
///
/// Preserves the byte-for-byte "produces the _after fixture" coverage while
/// dry-run keeps stdout empty.
fn reorder_in_place(path: &std::path::Path, ext: &str) -> String {
    let tmp = temp_file_ext(ext);
    fs::copy(path, &tmp).unwrap();
    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "in-place reorder failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    result
}

/// Run rust-llm-tidy on `content` (written to a tempfile) with optional
/// `--dry-run`.
/// Returns (stdout, stderr, exit_code).
fn run(content: &str, args: &[&str]) -> (String, String, i32) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file = dir.join(format!("rust-llm-tidy-test-{}-{}.rs", pid, seq));
    fs::write(&file, content).unwrap();

    let mut full_args = vec!["--include", "reorder"];
    full_args.extend(args);
    let output = run_command(&full_args, &file);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit = output.status.code().unwrap_or(-1);

    let _ = fs::remove_file(&file);

    (stdout, stderr, exit)
}

/// Read a tempfile after rust-llm-tidy has modified it.
fn run_and_read(content: &str) -> String {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file = dir.join(format!("rust-llm-tidy-test-{}-{}.rs", pid, seq));
    fs::write(&file, content).unwrap();

    let output = run_command(&["--include", "reorder"], &file);
    assert!(
        output.status.success(),
        "rust-llm-tidy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result = fs::read_to_string(&file).unwrap();
    let _ = fs::remove_file(&file);
    result
}

/// Run `rust-llm-tidy` on a directory with optional arguments.
fn run_dir(dir: &std::path::Path, args: &[&str]) -> (String, String, i32) {
    let mut full_args = vec!["--include", "reorder"];
    full_args.extend(args);
    let output = run_command(&full_args, dir);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit)
}

/// Run `rust-llm-tidy --include reorder --dry-run` on `path` and return
/// `(stdout, stderr, exit)`.
fn run_dry_run(path: &std::path::Path) -> (String, String, i32) {
    let output = run_command(&["--include", "reorder", "--dry-run"], path);

    assert!(
        output.status.success(),
        "rust-llm-tidy --dry-run failed on {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Strip the `path:` label from every stderr line so outputs for the same
/// content under different file names compare equal.
fn strip_path_prefix(stderr: &str, path: &std::path::Path) -> String {
    let prefix = format!("{}:", path.display());
    stderr
        .lines()
        .map(|line| line.strip_prefix(&prefix).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary `.rs` file path for fixture copies that
/// reorder in place.
fn temp_file() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-file-{}-{}.rs", pid, seq))
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(["--no-config"]).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Create a numbered temporary file path with the given extension.
///
/// Fixture copies use the extension to select the language (`.cs`);
/// case-sensitivity tests need `.RS`/`.MD`/`.TXT` (the local `temp_file`
/// is fixed to `.rs`).
fn temp_file_ext(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-ext-{}-{}.{}", pid, seq, ext))
}
