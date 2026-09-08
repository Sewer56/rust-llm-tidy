//! DOC004 missing `# Arguments` sections over the Rust fixtures.
//!
//! Every test runs `--include DOC004` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `doc004_missing_arguments.rs` warns on pub fns with parameters but no
/// `# Arguments` section.
#[test]
fn doc004_missing_arguments() {
    let (stderr, exit) = run_rust_fixture("doc004_missing_arguments.rs", "DOC004");

    // DOC004 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC004 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC004", Some("greet"));

    let doc004_count = stderr.matches("DOC004").count();
    assert_eq!(
        doc004_count, 1,
        "expected exactly 1 DOC004 finding, got {doc004_count}:\n{stderr}"
    );
}

/// `no_args` has no parameters and is not flagged by DOC004.
#[test]
fn doc004_no_params_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc004_missing_arguments.rs", "DOC004");
    assert!(
        !stderr.contains("no_args"),
        "fn with no parameters should not be flagged:\n{stderr}"
    );
}
