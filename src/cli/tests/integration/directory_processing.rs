//! Recursive directory processing tests for the `rust-llm-tidy` CLI.
//!
//! Every test writes a temp directory tree, runs the CLI over its root,
//! and asserts on the per-file reports and written content.

use super::{run_dir, temp_dir};
use std::fs;

/// Directory recursion collects `README.MD`/`lib.RS` case variants and
/// excludes extensions outside the default set (like `notes.org`).
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
    fs::write(dir.join("notes.org"), "not allowed by default\n").unwrap();

    // Rust-only reorder runs on the nested `.RS` and reports it by path.
    let (_stdout, stderr, exit) = run_dir(&dir, &["--dry-run"]);
    assert_eq!(exit, 1, "dry-run must fail for proposed Rust changes");
    assert!(
        stderr.contains("lib.RS"),
        "recursion must collect and process lib.RS: {stderr}"
    );

    // Markdown table fix runs on the `.MD` and reports it by path.
    let (_stdout, md_stderr, md_exit) = run_dir(&dir, &["--include", "tables", "--dry-run"]);
    assert_eq!(md_exit, 1, "dry-run must fail for proposed table changes");
    assert!(
        md_stderr.contains("README.MD") && md_stderr.contains("success[FIX]"),
        "recursion must collect and process README.MD: {md_stderr}"
    );
    assert!(
        !md_stderr.contains("notes.org") && !stderr.contains("notes.org"),
        "notes.org must be excluded silently"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--dry-run` on a directory reports each file's moves as path-labeled change
/// lines on stderr, leaving stdout empty and the files unmodified.
#[test]
fn recursive_directory_dry_run_should_label_each_move_with_path() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();

    let file_a = dir.join("a.rs");
    let file_b = dir.join("b.rs");

    fs::write(&file_a, "fn a() {}\nfn b() { a(); }\n").unwrap();
    fs::write(&file_b, "fn c() {}\nfn d() { c(); }\n").unwrap();

    let (stdout, stderr, exit) = run_dir(&dir, &["--dry-run"]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit, 1, "dry-run must fail for proposed directory changes");
    assert!(stdout.is_empty(), "text dry-run must leave stdout empty");
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
fn recursive_directory_error_should_still_reorder_valid_file() {
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
