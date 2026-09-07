//! DOC005 undocumented parameters over the Rust fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `render` documents both parameters and is not flagged by DOC005.
#[test]
fn doc005_documented_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc005_undocumented_param.rs");
    assert!(
        !stderr.contains("render"),
        "fn with complete # Arguments should not be flagged:\n{stderr}"
    );
}

/// `doc005_undocumented_param.rs` warns when a `# Arguments` section omits a
/// parameter name.
#[test]
fn doc005_undocumented_param() {
    let (stderr, exit) = run_rust_fixture("doc005_undocumented_param.rs");

    // DOC005 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC005 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC005", Some("build"));
    assert!(
        stderr.contains("fmt"),
        "DOC005 should mention the undocumented param `fmt`:\n{stderr}"
    );

    let doc005_count = stderr.matches("DOC005").count();
    assert_eq!(
        doc005_count, 1,
        "expected exactly 1 DOC005 finding, got {doc005_count}:\n{stderr}"
    );
}
