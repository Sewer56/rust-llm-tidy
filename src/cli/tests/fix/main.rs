//! Integration tests for the `fix` subcommand of `rust-llm-tidy`.
//!
//! Mirrors the helper pattern from `doc_check/mod.rs` (`run_command`,
//! `manifest_dir`, `fixture_dir`; `binary` lives in the shared `common`
//! module).
//!
//! Tests run the built CLI binary against fixtures in
//! `tests/fixtures/fix/` and hermetic temporary source files.
//!
//! The shared helpers and the unique-name counter live here; child modules:
//! - `command_behavior`: exit codes, change records, and in-place writes.
//! - `fences`: fence-fix dry-run reporting.
//! - `links`: link hoisting, thresholds, CRLF, and the intra-doc repro.
//! - `literal_safety`: literal and config bytes stay untouched by fixes.
//! - `tables`: per-language table realignment boundaries.

use common::binary;
use core::sync::atomic::{AtomicU64, Ordering};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};

mod command_behavior;
mod fences;
mod links;
mod literal_safety;
mod tables;
// The folder root sits inside `tests/fix/`, so the helpers shared by every
// test binary resolve at their sibling path, not under this folder.
#[path = "../common/mod.rs"]
mod common;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// -- Helpers (mirrors doc_check/mod.rs) -------------------------------

/// The directory holding `fix` fixtures.
fn fixture_dir() -> PathBuf {
    manifest_dir().join("tests").join("fixtures").join("fix")
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &Path) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join(".rust-llm-tidy.yml");
    fs::write(&config, "{}\n").unwrap();

    let mut cmd = Command::new(binary());
    cmd.arg("--config").arg(config).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Create a numbered temporary directory.
fn temp_dir() -> PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-fix-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary file path.
fn temp_file(ext: &str) -> PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-fix-{}-{}.{}", pid, seq, ext))
}

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
