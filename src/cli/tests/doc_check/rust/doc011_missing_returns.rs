//! DOC011 missing `# Returns` sections over the Rust fixtures.
//!
//! Every test runs `--include DOC011` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `label`, `reset`, `builder`, and `hidden` are not flagged by DOC011.
#[test]
fn doc011_clean_cases_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc011_missing_returns.rs", "DOC011");

    for name in ["label", "reset", "builder", "hidden"] {
        assert!(
            !stderr.contains(name),
            "`{name}` should not be flagged:\n{stderr}"
        );
    }
}

/// `doc011_missing_returns.rs` warns on pub fns returning a value and
/// reminds on bool returns when no `# Returns` section exists.
#[test]
fn doc011_value_fns_warn_and_bool_fns_remind() {
    let (stderr, exit) = run_rust_fixture("doc011_missing_returns.rs", "DOC011");

    // DOC011 warnings and reminders should not fail the run.
    assert_eq!(exit, 0, "DOC011 findings should not fail the run");

    assert!(
        stderr.contains("warning[DOC011]"),
        "expected a warning:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[DOC011]"),
        "expected a reminder:\n{stderr}"
    );
    assert_has_diagnostic(&stderr, "DOC011", Some("double"));
    assert_has_diagnostic(&stderr, "DOC011", Some("is_ready"));

    let doc011_count = stderr.matches("DOC011").count();
    assert_eq!(
        doc011_count, 2,
        "expected exactly 2 DOC011 findings, got {doc011_count}:\n{stderr}"
    );
}
