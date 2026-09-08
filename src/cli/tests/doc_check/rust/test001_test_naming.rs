//! TEST001 discouraged test-function names over the Rust fixtures.
//!
//! Every test runs `--include TEST001` on `test001_test_naming.rs` and
//! asserts on its exit code and stderr diagnostics. The shared runner
//! helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `should_pass_when_valid` is a behavioral name and is not flagged.
#[test]
fn test001_behavioral_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("test001_test_naming.rs", "TEST001");
    assert!(
        !stderr.contains("should_pass_when_valid"),
        "behavioral test name should not be flagged:\n{stderr}"
    );
}

/// `test001_test_naming.rs` warns on discouraged test-function names.
#[test]
fn test001_test_naming() {
    let (stderr, exit) = run_rust_fixture("test001_test_naming.rs", "TEST001");

    // TEST001 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "TEST001 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "TEST001", Some("test_foo"));
    assert_has_diagnostic(&stderr, "TEST001", Some("test1"));
    assert_has_diagnostic(&stderr, "TEST001", Some("case_1"));

    let test001_count = stderr.matches("TEST001").count();
    assert_eq!(
        test001_count, 3,
        "expected exactly 3 TEST001 findings, got {test001_count}:\n{stderr}"
    );
}
