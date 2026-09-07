//! TEXT003 over-budget sentences in Python docstrings.
//!
//! The test runs `--include lints` on a fixture under
//! `tests/fixtures/doc/python/` and asserts on its exit code and stderr
//! diagnostics.

use crate::run_python_fixture;

/// Python docstring sentences over the word budget warn with TEXT003.
///
/// The long sentence reports at the module docstring's first prose line, the
/// function docstring stays quiet, and warnings leave the exit code at 0.
#[test]
fn py_docstring_long_sentence_warns_text003() {
    let (stderr, exit) = run_python_fixture("docstring_sentence_budgets.py");

    assert_eq!(exit, 0, "TEXT003 warnings must not fail the run:\n{stderr}");
    assert!(
        stderr.contains(":2: warning[TEXT003]"),
        "TEXT003 must report at the docstring sentence's first prose line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT003").count(),
        1,
        "the module docstring sentence only, never the function docstring:\n{stderr}"
    );
}
