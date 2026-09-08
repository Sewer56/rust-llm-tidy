//! Integration tests for the `check` and `all` subcommands of
//! `rust-llm-tidy`.
//!
//! These are kept separate from the reorder integration tests so the
//! documentation-lint behavior is exercised in isolation.
//!
//! Each test runs the built CLI binary against a fixture or temp file and
//! asserts on its exit code and stderr diagnostics.
//!
//! Child modules:
//! - `command_behavior`: `all` fix behavior on markdown files
//! - `comment_lexicons`: comment-family text budgets across languages
//! - `csharp`: C# XML-doc lints over the `.cs` fixtures
//! - `json_output`: `--output-mode json` record contracts
//! - `mod001_module_size`: cross-language MOD001 acceptance
//! - `python`: Python lints over the `.py` fixtures
//! - `rust`: Rust lints over the `.rs` fixtures
//!
//! Text-budget and wording modules:
//!
//! - `text001_paragraph_size`: TEXT001 paragraph budgets (markdown, Python)
//! - `text002_line_length`: TEXT002 over-long markdown lines
//! - `text003_sentence_length`: TEXT003 long Python docstring sentences
//! - `text004_header_opener`: TEXT004 three-sentence markdown openers
//! - `text006_verbose_synonyms`: TEXT006 wording hints in markdown
//! - `text007_passive_narration`: TEXT007 narration and passive-voice hints
//! - `text008_list_density`: TEXT008 list budgets and CLI rule selection
//!
//! The shared setup (diagnostic assertion, runners, and fixture/temp
//! helpers) lives at the bottom of this file.

use common::binary;
use core::sync::atomic::{AtomicU64, Ordering};
use std::fs;
use std::process::Command;

mod command_behavior;
mod comment_lexicons;
mod csharp;
mod json_output;
mod mod001_module_size;
mod python;
mod rust;
mod text001_paragraph_size;
mod text002_line_length;
mod text003_sentence_length;
mod text004_header_opener;
mod text006_verbose_synonyms;
mod text007_passive_narration;
mod text008_list_density;
// The folder root sits inside `tests/doc_check/`, so the helpers shared by
// every test binary resolve at their sibling path, not under this folder.
#[path = "../common/mod.rs"]
mod common;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// ── `all` subcommand ──────────────────────────────────────────────

/// `all` on a clean file passes with no diagnostics.
#[test]
fn all_clean_file() {
    let path = rust_fixture_dir().join("clean.rs");
    let output = run_command(&["--dry-run"], &path);

    assert!(
        output.status.success(),
        "all on clean file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Error handling ────────────────────────────────────────────────

/// `all` on a file with doc gaps reports them after reordering.
#[test]
fn all_reports_remaining_doc_gaps() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("gap.rs");
    std::fs::write(&file, "pub fn undocumented() {}\n").unwrap();

    let output = run_command(&[], &file);

    assert!(
        !output.status.success(),
        "all should fail on remaining doc gaps"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DOC001"),
        "all should report doc gaps, got:\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Assert that `stderr` contains the given diagnostic code and optional item
/// name.
fn assert_has_diagnostic(stderr: &str, code: &str, item_name: Option<&str>) {
    assert!(
        stderr.contains(code),
        "stderr should contain {code}, got:\n{stderr}"
    );
    if let Some(name) = item_name {
        assert!(
            stderr.contains(name),
            "stderr should mention `{name}`, got:\n{stderr}"
        );
    }
}

/// A non-existent path is rejected.
#[test]
fn check_nonexistent_path_fails() {
    let nonexistent = std::env::temp_dir().join(format!(
        "rust-llm-tidy-lint-missing-{}-{}.rs",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let output = run_command(&["--include", "lints"], &nonexistent);

    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
}

// ── Directory recursion ───────────────────────────────────────────

/// `check` descends into directories recursively.
#[test]
fn check_recursive_directory() {
    let dir = temp_dir();
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    // Clean file at the root.
    std::fs::copy(rust_fixture_dir().join("clean.rs"), dir.join("clean.rs")).unwrap();
    // Undocumented file in a nested dir.
    std::fs::write(sub.join("dirty.rs"), "pub fn dirty() {}\n").unwrap();

    let output = run_command(&["--include", "lints"], &dir);

    assert!(
        !output.status.success(),
        "directory with missing docs should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DOC001") && stderr.contains("dirty.rs"),
        "should flag the nested undocumented file, got:\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Helpers ───────────────────────────────────────────────────────

/// The directory holding the default-run mixed-language fixtures.
fn defaults_fixture_dir() -> std::path::PathBuf {
    fixture_dir().join("defaults")
}

/// The directory holding fix fixtures.
fn fix_fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("fix")
}

/// A markdown paragraph over 240 chars built from short (under-80) lines, so
/// only TEXT001 fires on it.
fn oversized_paragraph_md() -> String {
    let lines: String = (0..10)
        .map(|i| format!("sentence number {i} carries some filler text\n"))
        .collect();
    format!("# Title\n\n{lines}\nTrailer.\n")
}

/// The reorder fixture root; callers join the language dir (`rust` or
/// `csharp`) before the fixture name.
fn reorder_fixture_dir() -> std::path::PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder")
}

/// Run `rust-llm-tidy --include lints` on a lexicon-family fixture and
/// return its (stderr, exit_code).
fn run_lexicon_fixture(name: &str) -> (String, i32) {
    let path = fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run `rust-llm-tidy --include lints` on a Python fixture and return its
/// (stderr, exit_code).
fn run_python_fixture(name: &str) -> (String, i32) {
    let path = python_fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Writes `content` to a numbered temp `.md` file and returns its path.
fn temp_md(content: &str) -> std::path::PathBuf {
    let path = temp_file("md");
    fs::write(&path, content).unwrap();
    path
}

/// Write `content` to `rel` (a relative path inside a fresh temp dir)
/// and return the file's path; parent directories are created.
fn temp_named_file(rel: &str, content: &str) -> std::path::PathBuf {
    let path = temp_dir().join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
    path
}

/// A markdown file whose line 1 carries a narration marker and line 2 a
/// passive construction, so both TEXT007 classes are observable.
fn text007_marker_and_passive_md() -> String {
    "This no longer panics.\nErrors are returned by the scanner.\n".to_string()
}

/// The directory holding the Python lint fixtures.
fn python_fixture_dir() -> std::path::PathBuf {
    fixture_dir().join("python")
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(["--no-config"]).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// The directory holding the Rust lint fixtures.
fn rust_fixture_dir() -> std::path::PathBuf {
    fixture_dir().join("rust")
}

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-lint-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary file path with the given extension.
fn temp_file(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-all-{}-{}.{}", pid, seq, ext))
}

/// The directory holding the shared, cross-language lint fixtures.
fn fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("doc")
}

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
