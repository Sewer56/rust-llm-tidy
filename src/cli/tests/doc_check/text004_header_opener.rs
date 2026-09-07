//! TEXT004 three-sentence heading openers in markdown.
//!
//! The test writes a temp markdown file, runs the built CLI binary, and
//! asserts on its exit code and stderr diagnostics.

use crate::{run_command, temp_md};

/// A three-sentence markdown heading opener warns with TEXT004 at the
/// paragraph's first line.
///
/// The one-sentence heading opener in the same file stays silent, and
/// warnings keep the exit code at 0.
#[test]
fn md_three_sentence_heading_opener_warns_text004_without_failing() {
    let opener = "Apples grow on trees. They are quite tasty. Pick them in fall.";
    let path = temp_md(&format!(
        "# Fruits\n\n{opener}\n\n# Notes\n\nOne sentence about notes.\n"
    ));
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT004 warnings must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":3: warning[TEXT004]"),
        "expected a TEXT004 warning at the heading opener's first line, got:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT004").count(),
        1,
        "exactly the three-sentence opener, never the one-sentence one:\n{stderr}"
    );
}
