//! Integration tests for `rust-llm-tidy` CLI.
//!
//! Tests are split into two groups:
//!
//! 1. Synthetic fixture tests (`tests/fixtures/reorder/*_before.rs` → `*_after.rs`):
//!    one test per ordering/spacing rule.  Each fixture's module header
//!    documents the rule and the expected before/after state.
//!
//! 2. CLI behavior tests: dry-run, in-place writes, directory traversal,
//!    error handling, and idempotency.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Run `rust-llm-tidy --include reorder --dry-run` against `<name>_before.rs` in `tests/fixtures/reorder/`.
///
/// Returns `(stdout, stderr, exit, before_path, expected_after_content)`.
macro_rules! run_fixture {
    ($name:ident) => {{
        let fixture_dir = manifest_dir()
            .join("tests")
            .join("fixtures")
            .join("reorder");
        let before_path = fixture_dir.join(concat!(stringify!($name), "_before.rs"));
        let expected_after =
            include_str!(concat!("fixtures/reorder/", stringify!($name), "_after.rs")).to_string();

        let (stdout, stderr, exit) = run_dry_run(&before_path);

        (stdout, stderr, exit, before_path, expected_after)
    }};
}

/// Declare a fixture test.  The test name is the fixture rule name.
///
/// Dry-run reports change records on stderr (allowing zero records for an
/// already-tidy fixture) and keeps stdout empty. Byte-for-byte "produces
/// _after" coverage is preserved by re-ordering a temp copy in place and
/// comparing its content.
macro_rules! synthetic_fixture {
    ($name:ident) => {
        #[test]
        fn $name() {
            let (stdout, stderr, exit, before_path, expected_after) = run_fixture!($name);
            assert_eq!(
                exit, 0,
                concat!(stringify!($name), " dry-run should succeed")
            );
            assert!(
                stdout.is_empty(),
                concat!(
                    stringify!($name),
                    " dry-run must not print reconstructed source to stdout"
                )
            );
            // Any stderr output from a reorder dry-run must be change records,
            // never reconstructed source (which would lack the op marker).
            for line in stderr.lines() {
                assert!(
                    line.contains("success[REORDER]"),
                    "{} dry-run stderr must only carry change records: {}",
                    stringify!($name),
                    line
                );
            }
            // In-place reorder still produces the _after fixture byte-for-byte.
            assert_eq!(
                reorder_in_place(&before_path),
                expected_after,
                concat!(
                    stringify!($name),
                    " fixture: in-place reorder must match _after.rs"
                )
            );
        }
    };
}

// ── Synthetic fixture tests: one per rule ─────────────────────────

synthetic_fixture!(phase_extern_crate_stable);

synthetic_fixture!(phase_other_stable);

synthetic_fixture!(phase_use_stable);

synthetic_fixture!(phase_mod_non_test_stable);

synthetic_fixture!(phase_macro_alphabetical);

synthetic_fixture!(phase_macro_dependency);

synthetic_fixture!(phase_macro_invocation_after_def);

synthetic_fixture!(phase_const_static_alphabetical);

synthetic_fixture!(phase_const_static_dependency);

synthetic_fixture!(phase_type_alphabetical);

synthetic_fixture!(phase_type_dependency);

synthetic_fixture!(phase_trait_alphabetical);

synthetic_fixture!(phase_trait_dependency);

synthetic_fixture!(phase_impl_inherent_before_trait);

synthetic_fixture!(phase_impl_after_matching_type);

synthetic_fixture!(phase_impl_orphan_stable);

synthetic_fixture!(fn_visibility_groups);

synthetic_fixture!(fn_main_first);

synthetic_fixture!(fn_callers_before_callees);

synthetic_fixture!(fn_alphabetical_tie_break);

synthetic_fixture!(fn_mutual_recursion_contiguous);

synthetic_fixture!(cfg_test_mod_last_stable);

synthetic_fixture!(preamble_preserved);

synthetic_fixture!(trailer_preserved);

synthetic_fixture!(fn_interstitial_comment_travels_with_next);

synthetic_fixture!(docs_attrs_travel);

synthetic_fixture!(spacing_compact_use_mod_const_static);

synthetic_fixture!(spacing_blank_line_between_phases);

synthetic_fixture!(spacing_blank_line_fn_visibility);

synthetic_fixture!(safety_line_preservation);

// ── Idempotency: every _after.rs fixture must be unchanged ─────────

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Directory recursion collects `README.MD`/`lib.RS` case variants and
/// excludes unrelated extensions like `notes.txt`.
#[test]
fn recursive_dir_collects_uppercase_variants_excludes_others() {
    let dir = temp_dir();
    let nested = dir.join("src");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("lib.RS"), "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(
        dir.join("README.MD"),
        "| Name | Value | Description |\n| --- | --- | --- |\n| a | 1 | first |\n| longname | 200 | second item |\n",
    )
    .unwrap();
    fs::write(dir.join("notes.txt"), "not rust or markdown\n").unwrap();

    // Rust-only reorder runs on the nested `.RS` and reports it by path.
    let (_stdout, stderr, exit) = run_dir(&dir, &["--dry-run"]);
    assert_eq!(exit, 0, "dir with .RS/.MD/.txt should succeed");
    assert!(
        stderr.contains("lib.RS"),
        "recursion must collect and process lib.RS: {stderr}"
    );

    // Markdown table fix runs on the `.MD` and reports it by path.
    let (_stdout, md_stderr, md_exit) = run_dir(&dir, &["--include", "tables", "--dry-run"]);
    assert_eq!(md_exit, 0, "tables dry-run on dir should succeed");
    assert!(
        md_stderr.contains("README.MD") && md_stderr.contains("success[FIX]"),
        "recursion must collect and process README.MD: {md_stderr}"
    );
    assert!(
        !md_stderr.contains("notes.txt") && !stderr.contains("notes.txt"),
        "notes.txt must be excluded silently"
    );
    let _ = fs::remove_dir_all(&dir);
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

/// Reorder a temp copy of `path` in place and return the rewritten content.
///
/// Preserves the byte-for-byte "produces the _after fixture" coverage while
/// dry-run keeps stdout empty.
fn reorder_in_place(path: &std::path::Path) -> String {
    let tmp = temp_file();
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

/// In-place reorder of a CRLF source preserves every `\r\n` and reorders
/// callers before callees. CRLF input is built in-memory (not from a
/// committed fixture, which git would normalize on checkout).
#[test]
fn reorder_in_place_preserves_crlf() {
    let source = "fn b() { a(); }\r\nfn a() {}\r\n";
    let result = run_and_read(source);

    // Caller (b) before callee (a).
    let a_pos = result.find("fn a").expect("fn a missing");
    let b_pos = result.find("fn b").expect("fn b missing");
    assert!(b_pos < a_pos, "b (caller) before a (callee)");

    // Every `\n` must be part of `\r\n` (no CRLF -> LF flip).
    assert_eq!(
        result.matches('\n').count(),
        result.matches("\r\n").count(),
        "every newline must be CRLF after reorder: {result:?}"
    );
}

/// In-place reorder both writes the reordered file and reports the move as a
/// change line on stderr, mirroring the records a dry-run would have previewed.
#[test]
fn reorder_in_place_reports_change_and_writes() {
    let fixture = manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder");
    let expected =
        fs::read_to_string(fixture.join("fn_interstitial_comment_travels_with_next_after.rs"))
            .unwrap();
    let tmp = temp_file();
    fs::copy(
        fixture.join("fn_interstitial_comment_travels_with_next_before.rs"),
        &tmp,
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &tmp);
    assert!(
        output.status.success(),
        "in-place reorder on a moving fixture should succeed"
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, expected,
        "in-place reorder must write the after fixture"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[REORDER]"),
        "in-place reorder must also report its change line on stderr: {stderr}"
    );
}

/// Idempotency: every `_after.rs` fixture must be unchanged by a second run.
#[test]
fn test_all_after_fixtures_are_idempotent() {
    let fixture_dir = manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder");
    let mut after_files: Vec<_> = fs::read_dir(&fixture_dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if name.ends_with("_after.rs") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    after_files.sort();

    assert!(!after_files.is_empty(), "no _after.rs fixtures found");

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

// ── CLI behavior tests ────────────────────────────────────────────

/// `--dry-run` reports the would-be reorder move on stderr without printing
/// the reconstructed source to stdout or modifying the file on disk.
#[test]
fn test_dry_run() {
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
fn test_empty_directory() {
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

/// In-place write: copy a synthetic before fixture to a temp file, run without
/// `--dry-run`, and verify the file content matches the after fixture.
#[test]
fn test_in_place_write() {
    let expected = include_str!("fixtures/reorder/phase_use_stable_after.rs");

    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("rust-llm-tidy-write-test-{}-{}.rs", pid, seq));

    fs::write(
        &tmp,
        include_str!("fixtures/reorder/phase_use_stable_before.rs"),
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

/// A non-existent path is rejected with an error exit.
#[test]
fn test_nonexistent_path() {
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

/// A directory is processed recursively, reordering every `.rs` file.
#[test]
fn test_recursive_directory() {
    let dir = temp_dir();
    let root_file = dir.join("phase_use.rs");
    let nested_dir = dir.join("utils");
    let nested_file = nested_dir.join("phase_mod.rs");

    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(
        &root_file,
        include_str!("fixtures/reorder/phase_use_stable_before.rs"),
    )
    .unwrap();
    fs::write(
        &nested_file,
        include_str!("fixtures/reorder/phase_mod_non_test_stable_before.rs"),
    )
    .unwrap();

    let output = run_command(&["--include", "reorder"], &dir);
    assert!(
        output.status.success(),
        "directory run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected_root = include_str!("fixtures/reorder/phase_use_stable_after.rs");
    let expected_nested = include_str!("fixtures/reorder/phase_mod_non_test_stable_after.rs");
    let actual_root = fs::read_to_string(&root_file).unwrap();
    let actual_nested = fs::read_to_string(&nested_file).unwrap();

    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        actual_root, expected_root,
        "phase_use.rs should be reordered in place"
    );
    assert_eq!(
        actual_nested, expected_nested,
        "utils/phase_mod.rs should be reordered in place"
    );
}

/// `--dry-run` on a directory reports each file's moves as path-labeled change
/// lines on stderr, leaving stdout empty and the files unmodified.
#[test]
fn test_recursive_directory_dry_run() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let file_a = dir.join("a.rs");
    let file_b = dir.join("b.rs");

    fs::write(&file_a, "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(&file_b, "fn c() {}\nfn d() { c(); }\n").unwrap();

    let (stdout, stderr, exit) = run_dir(&dir, &["--dry-run"]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit, 0, "dry-run on directory should succeed");
    assert!(stdout.is_empty(), "stdout should be empty on success");
    assert!(
        stderr.contains("a.rs:") && stderr.contains("b.rs:"),
        "multi-file dry-run must label each change line with its path: {stderr}"
    );
    assert!(
        stderr.contains("rearrange fn b from pos 2 to pos 1")
            && stderr.contains("rearrange fn d from pos 2 to pos 1"),
        "directory dry-run should report each file's move on stderr: {stderr}"
    );
}

/// If a directory contains a valid file and an invalid file, the valid file is
/// still reordered and the operation exits non-zero.
#[test]
fn test_recursive_directory_error_continues() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let good = dir.join("good.rs");
    let bad = dir.join("bad.rs");

    fs::write(&good, "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(&bad, "not valid rust {{{").unwrap();

    let (_stdout, stderr, exit) = run_dir(&dir, &[]);

    let actual_good = fs::read_to_string(&good).unwrap();
    let _ = fs::remove_dir_all(&dir);

    assert_ne!(exit, 0, "directory with invalid file should exit non-zero");
    assert!(
        !stderr.is_empty(),
        "stderr should contain error message for invalid file"
    );

    let a_pos = actual_good.find("fn a").expect("fn a missing");
    let b_pos = actual_good.find("fn b").expect("fn b missing");
    assert!(
        b_pos < a_pos,
        "valid file should still be reordered despite sibling error"
    );
}

/// Realistic file: struct, impl, use, and multiple fns.
/// Tests that multi-phase ordering keeps use first, struct+impl together, and
/// callers before callees.
#[test]
fn test_reorder_real_file() {
    let source = "\
use std::fmt;\n\n\
pub struct Config {\n\
    pub name: String,\n}\n\n\
impl Config {\n\
    pub fn new(name: &str) -> Self {\n\
        Config {\n\
            name: name.to_string(),\n\
        }\n\
    }\n}\n\n\
fn validate(c: &Config) -> bool {\n\
    !c.name.is_empty()\n}\n\n\
pub fn build(name: &str) -> Option<Config> {\n\
    let c = Config::new(name);\n\
    if validate(&c) {\n\
        Some(c)\n\
    } else {\n\
        None\n\
    }\n}\n";

    let result = run_and_read(source);

    let use_pos = result.find("use std::fmt").unwrap();
    let struct_pos = result.find("pub struct Config").unwrap();
    let impl_pos = result.find("impl Config").unwrap();
    let build_pos = result.find("pub fn build").unwrap();
    let validate_pos = result.find("fn validate").unwrap();

    assert!(use_pos < struct_pos, "use before struct");
    assert!(struct_pos < impl_pos, "struct before its impl");
    assert!(
        build_pos < validate_pos,
        "build (caller) before validate (callee)"
    );
}

/// An already-sorted file (callers before callees) should be unchanged.
#[test]
fn test_roundtrip_sorted() {
    let source = "\
fn main() {\n\
    a();\n\
    b();\n}\n\n\
fn a() {\n\
    helper();\n}\n\n\
fn b() {}\n\n\
fn helper() {}\n";

    let result = run_and_read(source);
    let main_pos = result.find("fn main").unwrap();
    let a_pos = result.find("fn a").unwrap();
    let b_pos = result.find("fn b").unwrap();
    let helper_pos = result.find("fn helper").unwrap();

    assert!(main_pos < a_pos, "main before a");
    assert!(a_pos < helper_pos, "a before helper (a calls helper)");
    assert!(b_pos < helper_pos, "b before helper (original order)");
}

/// Safety check: corrupted output (missing lines) must cause an error exit.
/// We verify that rust-llm-tidy exits non-zero when given a non-Rust file.
#[test]
fn test_safety_aborts() {
    let source = "not valid rust {{{";

    let (_stdout, stderr, exit) = run(source, &[]);

    assert_ne!(exit, 0, "rust-llm-tidy should exit non-zero on parse error");
    assert!(!stderr.is_empty(), "stderr should contain error message");
}

/// An explicit `Note.MD` file is admitted, runs markdown fix ops, and
/// never runs the Rust-only reorder op.
#[test]
fn uppercase_md_explicit_file_runs_fix_not_rust_ops() {
    let file = temp_file_ext("MD");
    fs::write(
        &file,
        "| Name | Value | Description |\n| --- | --- | --- |\n| a | 1 | first |\n| longname | 200 | second item |\n",
    )
    .unwrap();

    // Tables (a markdown fix op) run on the `.MD` file.
    let output = run_command(&["--include", "tables"], &file);
    assert!(
        output.status.success(),
        ".MD file should be admitted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        ".MD must run markdown fix ops: {stderr}"
    );

    // Reorder (a Rust-only op) never runs on a `.MD` file, even when the bytes
    // would reorder as Rust.
    let output = run_command(&["--include", "reorder", "--dry-run"], &file);
    assert!(
        output.status.success(),
        ".MD reorder dry-run should succeed without Rust ops: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("success[REORDER]"),
        ".MD must never run the Rust reorder op"
    );
    let _ = fs::remove_file(&file);
}

// ── Case-insensitive extension admission ───────

/// An explicit `Foo.RS` file is admitted and runs the Rust reorder op,
/// matching the lowercase `.rs` behavior.
#[test]
fn uppercase_rs_explicit_file_runs_reorder() {
    let file = temp_file_ext("RS");
    fs::write(&file, "fn a() {}\nfn b() { a(); }\n").unwrap();

    let output = run_command(&["--include", "reorder"], &file);
    let _ = fs::remove_file(&file);

    assert!(
        output.status.success(),
        ".RS file should be admitted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[REORDER]"),
        ".RS must run the Rust reorder op: {stderr}"
    );
}

/// An explicit `.TXT` file is a silent skip: exit 0 and `[]` in JSON mode.
#[test]
fn uppercase_txt_explicit_file_is_silently_skipped() {
    let file = temp_file_ext("TXT");
    fs::write(&file, "not rust or markdown\n").unwrap();

    let output = run_command(&["--json"], &file);
    let _ = fs::remove_file(&file);

    assert!(
        output.status.success(),
        "unadmitted .TXT file must succeed silently: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "[]",
        ".TXT explicit file is a silent skip in JSON mode"
    );
}

// ── Helpers ───────────────────────────────────────────────────────

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run rust-llm-tidy on `content` (written to a tempfile) with optional `--dry-run`.
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

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary `.rs` file path (reorder only runs on Rust
/// inputs, so the temp copy must keep the extension).
fn temp_file() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-file-{}-{}.rs", pid, seq))
}

/// Create a numbered temporary file path with the given extension, for
/// case-sensitivity tests that need `.RS`/`.MD`/`.TXT` (the local `temp_file`
/// is fixed to `.rs`).
fn temp_file_ext(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-ext-{}-{}.{}", pid, seq, ext))
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(["--no-config"]).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Return the path to the `rust-llm-tidy` debug binary.
///
/// Prefers `CARGO_BIN_EXE_rust_llm_tidy` (set by `cargo test` at runtime);
/// falls back to the sibling of the test binary under `target/<triple>/debug/`,
/// since the test binary lives in `target/<triple>/debug/deps/`.
fn binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy") {
        return std::path::PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("current_exe must resolve");
    // Drop `<test-name>-<hash>` and `deps/` to reach `target/<triple>/debug/`.
    path.pop();
    path.pop();
    path.join("rust-llm-tidy")
}
