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
