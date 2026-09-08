//! TEXT001 over-budget Rust doc paragraphs.
//!
//! The test runs the built CLI binary on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;

/// Rust block and attribute doc prose fires the text budgets with
/// original file lines.
///
/// - TEXT001 errors on the over-budget `/** */` and `#[doc = "..."]`
///   paragraphs.
/// - TEXT002 warns on the 81-char block and attribute lines.
#[test]
fn rs_block_and_attribute_docs_fire_text_budgets() {
    let (stderr, exit) = run_rust_fixture(
        "text-001_text-002_block_attr_budgets.rs",
        "TEXT001,TEXT002,DOC001",
    );

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the block doc's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":25: error[TEXT001]"),
        "TEXT001 must report at the attribute paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":8: warning[TEXT002]"),
        "TEXT002 must report at the over-long attribute line:\n{stderr}"
    );
    assert!(
        stderr.contains(":12: warning[TEXT002]"),
        "TEXT002 must report at the over-long block doc line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "exactly the block and attribute paragraphs, never the plain block:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        2,
        "exactly the block and attribute lines, never the plain block:\n{stderr}"
    );
    assert!(
        !stderr.contains("DOC001"),
        "the fixture's private fns stay out of DOC001:\n{stderr}"
    );
}
