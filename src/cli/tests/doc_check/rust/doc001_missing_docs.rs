//! DOC001 missing doc comments over the Rust fixtures.
//!
//! Every test runs `--include DOC001` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// The documented function in `doc001_missing_docs.rs` is not flagged.
#[test]
fn doc001_documented_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc001_missing_docs.rs", "DOC001");
    assert!(
        !stderr.contains("`documented`"),
        "documented functions should not be flagged:\n{stderr}"
    );
}

/// `doc001_missing_docs.rs` flags every undocumented non-private documentable
/// item, skips private items and `pub use`.
#[test]
fn doc001_missing_docs() {
    let (stderr, exit) = run_rust_fixture("doc001_missing_docs.rs", "DOC001");
    assert_ne!(exit, 0, "missing docs should fail");

    // Every undocumented public item is flagged.
    assert_has_diagnostic(&stderr, "DOC001", Some("alpha"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Beta"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Gamma"));
    assert_has_diagnostic(&stderr, "DOC001", Some("DELTA"));
    assert_has_diagnostic(&stderr, "DOC001", Some("EPSILON"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Zeta"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Eta"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Theta"));

    // Count DOC001 occurrences: 8 expected.
    let doc001_count = stderr.matches("DOC001").count();
    assert_eq!(
        doc001_count, 8,
        "expected exactly 8 DOC001 findings, got {doc001_count}:\n{stderr}"
    );
}

/// The private function in `doc001_missing_docs.rs` is not flagged.
#[test]
fn doc001_private_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc001_missing_docs.rs", "DOC001");
    assert!(
        !stderr.contains("`helper`"),
        "private functions should not be flagged:\n{stderr}"
    );
}

/// `pub use` is not a documentable kind and must not be flagged.
#[test]
fn doc001_pub_use_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc001_missing_docs.rs", "DOC001");
    // The diagnostic kind for a use item would be `(use)`; verify it never
    // appears. Using a bare "use" substring would false-match the file path.
    assert!(
        !stderr.contains("(use)"),
        "pub use should not be flagged:\n{stderr}"
    );
}
