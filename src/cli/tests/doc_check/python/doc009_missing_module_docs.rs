//! Missing Python module-docstring diagnostics over the Python fixtures.
//!
//! Every test runs the built CLI binary and asserts on its exit code and
//! stderr diagnostics.

use crate::{python_fixture_dir, run_command};

/// Missing docstrings fail with purpose-first guidance, not Rust layout advice.
#[test]
fn doc009_should_explain_header_writing_when_module_docstring_is_missing() {
    let path = python_fixture_dir().join("doc009_missing_docstring.py");
    let expected = indoc::indoc! {"
        :1: error[DOC009]: module file is missing a module docstring.
        Fix: add a module docstring as the first statement.

        Help readers unfamiliar with the codebase understand the module's purpose
        without reading its implementation.
        - Read the module and relevant callers; document only supported facts.
        - Start with one concise sentence explaining what the module does and why.
          Do not just restate its name. A simple module needs no more.
        - If more detail is useful, put it below the summary, separated by a blank
          doc line. Outline major responsibilities, entry points, or non-obvious
          constraints. Use bullets for multiple topics.
        - Link to item docs instead of repeating their details. (file)"};

    let output = run_command(&["--include", "DOC009"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code().unwrap_or(-1),
        0,
        "a module without a docstring should fail"
    );
    assert!(
        stderr.contains(expected),
        "DOC009 must render its pinned line-1 diagnostic:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC009").count(),
        1,
        "one finding per module, never per statement:\n{stderr}"
    );
}
