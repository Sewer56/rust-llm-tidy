//! DOC002 missing `# Errors` sections over the Rust fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `save` in the fixture has a complete `# Errors` section and is not flagged.
#[test]
fn doc002_documented_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("save"),
        "fn with complete # Errors should not be flagged:\n{stderr}"
    );
}

/// `doc002_missing_errors_section.rs` flags pub fns returning Result without
/// a `# Errors` section.
#[test]
fn doc002_missing_errors_section() {
    let (stderr, exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert_ne!(exit, 0, "missing # Errors should fail");

    assert_has_diagnostic(&stderr, "DOC002", Some("load"));
    assert_has_diagnostic(&stderr, "DOC002", Some("fetch"));

    // Exactly 2 DOC002 findings (load and fetch).
    let doc002_count = stderr.matches("DOC002").count();
    assert_eq!(
        doc002_count, 2,
        "expected exactly 2 DOC002 findings, got {doc002_count}:\n{stderr}"
    );
}

/// `count` returns a non-Result type and is not flagged.
#[test]
fn doc002_non_result_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("count"),
        "non-Result pub fns should not be flagged:\n{stderr}"
    );
}

/// `load_private` is private and not flagged even though it returns Result.
#[test]
fn doc002_private_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("load_private"),
        "private fns should not be flagged:\n{stderr}"
    );
}
