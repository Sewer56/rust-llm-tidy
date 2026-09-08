//! TEXT002 over-long Rust doc lines.
//!
//! The test writes a temp Rust file, runs the built CLI binary, and
//! asserts on its exit code and stderr diagnostics.

use crate::{run_command, temp_file};
use std::fs;

/// Rust comments flow through the same text checks: an 81-char `///` line
/// warns with TEXT002 at the original source line.
#[test]
fn rs_long_doc_comment_warns_text002() {
    let path = temp_file("rs");
    fs::write(&path, format!("/// {}\nfn hidden() {{}}\n", "w".repeat(81))).unwrap();

    let output = run_command(&["--include", "TEXT002"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT002 warnings must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "expected a TEXT002 warning for the over-limit comment, got:\n{stderr}"
    );
}
