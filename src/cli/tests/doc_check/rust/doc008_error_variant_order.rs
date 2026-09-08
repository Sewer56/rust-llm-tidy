//! DOC008 out-of-order error variants over the Rust fixtures.
//!
//! Every test runs `--include DOC008` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `save` in the fixture lists variants alphabetically and is not flagged.
#[test]
fn doc008_sorted_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc008_error_variant_order.rs", "DOC008");
    assert!(
        !stderr.contains("save"),
        "fn with alphabetically listed variants should not be flagged:\n{stderr}"
    );
}

/// `doc008_error_variant_order.rs` errors when an `# Errors` section lists
/// the returned in-file enum's variants out of alphabetical order.
#[test]
fn doc008_variants_out_of_order() {
    let (stderr, exit) = run_rust_fixture("doc008_error_variant_order.rs", "DOC008");

    // DOC008 is error-severity - it must fail the run.
    assert_ne!(exit, 0, "out-of-order variants should fail the run");
    assert!(
        stderr.contains("error[DOC008]"),
        "DOC008 must render at error severity:\n{stderr}"
    );

    assert_has_diagnostic(&stderr, "DOC008", Some("load"));

    let doc008_count = stderr.matches("DOC008").count();
    assert_eq!(
        doc008_count, 1,
        "expected exactly 1 DOC008 finding, got {doc008_count}:\n{stderr}"
    );
}
