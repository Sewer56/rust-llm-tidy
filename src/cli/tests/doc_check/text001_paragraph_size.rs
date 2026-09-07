//! TEXT001 over-budget paragraphs in markdown and Python docstrings.
//!
//! Every test runs the built CLI binary on a temp markdown file or
//! Python fixture and asserts on its exit code and stderr diagnostics.

use crate::{oversized_paragraph_md, run_command, run_python_fixture, temp_md};

/// An over-limit markdown paragraph fails the run with a TEXT001 error; the
/// file is no longer skipped before linting.
#[test]
fn md_paragraph_over_limit_fails_with_text001() {
    let path = temp_md(&oversized_paragraph_md());
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "TEXT001 errors on markdown must fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":3: error[TEXT001]"),
        "expected a TEXT001 error at the paragraph's first line, got:\n{stderr}"
    );
}

/// `--exclude TEXT001` suppresses the markdown paragraph error; the run then
/// succeeds with no findings.
#[test]
fn md_text001_suppressed_by_exclude() {
    let path = temp_md(&oversized_paragraph_md());
    let output = run_command(&["--include", "lints", "--exclude", "TEXT001"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "excluding TEXT001 must clear the markdown error: {stderr}"
    );
    assert!(
        !stderr.contains("TEXT001"),
        "excluded code must not be reported, got:\n{stderr}"
    );
}

/// Python docstring prose fires the text budgets with original file lines.
///
/// TEXT001 errors on the module docstring's over-budget paragraph, and
/// TEXT002 warns on a function docstring's over-long line.
///
/// The non-docstring triple-quoted payload and the `>>>` doctest example
/// stay quiet.
#[test]
fn py_docstring_budgets_fire_with_original_lines() {
    let (stderr, exit) = run_python_fixture("docstring_text_budgets.py");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":2: error[TEXT001]"),
        "TEXT001 must report at the docstring's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":27: warning[TEXT002]"),
        "TEXT002 must report at the over-long docstring line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "the docstring paragraph only, never the payload:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "the wide docstring line only, never the doctest:\n{stderr}"
    );
}
