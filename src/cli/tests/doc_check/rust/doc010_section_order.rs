//! DOC010 out-of-standard-order doc sections over the Rust fixtures.
//!
//! Every test runs `--include DOC010` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `save` in the fixture lists sections in standard order and is not flagged.
#[test]
fn doc010_canonical_order_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc010_section_order.rs", "DOC010");
    assert!(
        !stderr.contains("save"),
        "fn with canonically ordered sections should not be flagged:\n{stderr}"
    );
}

/// `doc010_section_order.rs` errors when a public item's doc sections
/// appear out of standard order.
#[test]
fn doc010_sections_out_of_order() {
    let (stderr, exit) = run_rust_fixture("doc010_section_order.rs", "DOC010");

    // DOC010 is error-severity - it must fail the run.
    assert_ne!(exit, 0, "out-of-order sections should fail the run");
    assert!(
        stderr.contains("error[DOC010]"),
        "DOC010 must render at error severity:\n{stderr}"
    );

    assert_has_diagnostic(&stderr, "DOC010", Some("load"));

    let doc010_count = stderr.matches("DOC010").count();
    assert_eq!(
        doc010_count, 1,
        "expected exactly 1 DOC010 finding, got {doc010_count}:\n{stderr}"
    );
}
