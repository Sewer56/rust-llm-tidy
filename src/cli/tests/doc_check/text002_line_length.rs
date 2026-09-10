//! TEXT002 over-long markdown lines.
//!
//! The test writes a temp markdown file, runs the built CLI binary, and
//! asserts on its exit code and stderr diagnostics.

use crate::{run_command, temp_md};

/// A markdown line over 80 chars yields a TEXT002 warning without failing.
#[test]
fn md_long_line_warns_text002_without_failing() {
    let path = temp_md(&format!("{}\n", "x".repeat(81)));
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT002 warnings must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "expected a TEXT002 warning at line 1, got:\n{stderr}"
    );
}

/// Prose over the limit still warns when a URL ends the line; the reported
/// length excludes only the URL.
#[test]
fn md_long_prose_before_trailing_url_warns_text002() {
    let path = temp_md(&format!("{} https://example.com/x\n", "x".repeat(80)));
    let output = run_command(&["--include", "TEXT002"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "prose over the limit must still warn, got:\n{stderr}"
    );
    assert!(
        stderr.contains("line is 81 chars long."),
        "the reported length must exclude the trailing URL, got:\n{stderr}"
    );
}

/// A URL that is not at the end still counts, so the line warns.
#[test]
fn md_mid_line_url_warns_text002() {
    let path = temp_md(&format!(
        "see {} at https://example.com/x for details\n",
        "a".repeat(60)
    ));
    let output = run_command(&["--include", "TEXT002"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "a mid-line URL must count toward TEXT002, got:\n{stderr}"
    );
}

/// Prose glued to a closed markdown URL is not trailing, so it counts and
/// the line warns.
#[test]
fn md_prose_after_closed_url_warns_text002() {
    let source = format!("See <https://a.test>{}\n", "x".repeat(81));
    let path = temp_md(&source);
    let output = run_command(&["--include", "TEXT002"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "prose after a closed URL must count, got:\n{stderr}"
    );
}

/// A long URL that ends the line is excluded, so the line stays in budget
/// even though its raw length exceeds the limit.
#[test]
fn md_trailing_url_stays_within_budget() {
    let source = "See the reference at https://example.com/a/very/long/path/that/keeps/going/beyond/the/eighty/char/limit\n";
    let path = temp_md(source);
    let output = run_command(&["--include", "TEXT002"], &path);

    assert!(output.status.success(), "the trailing-URL case must exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("TEXT002"),
        "a trailing URL must not fire TEXT002, got:\n{stderr}"
    );
}
