//! DOC006 doc-comment placeholders over the Rust fixtures.
//!
//! Every test runs `--include DOC006` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// `done` has a clean doc comment and is not flagged by DOC006.
#[test]
fn doc006_clean_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc006_placeholders.rs", "DOC006");
    assert!(
        !stderr.contains("done"),
        "fn with clean doc should not be flagged:\n{stderr}"
    );
}

/// `doc006_placeholders.rs` warns on TODO/FIXME/TBD doc-comment markers.
#[test]
fn doc006_placeholders() {
    let (stderr, exit) = run_rust_fixture("doc006_placeholders.rs", "DOC006");

    // DOC006 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC006 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC006", Some("todo_task"));
    assert_has_diagnostic(&stderr, "DOC006", Some("fixme_task"));
    assert_has_diagnostic(&stderr, "DOC006", Some("tbd_task"));
    // A literal `...` is idiomatic prose, not a placeholder marker.
    assert!(
        !stderr.contains("(struct `Placeholder`)"),
        "ellipsis-only docs should not be flagged:\n{stderr}"
    );

    let doc006_count = stderr.matches("DOC006").count();
    assert_eq!(
        doc006_count, 3,
        "expected exactly 3 DOC006 findings, got {doc006_count}:\n{stderr}"
    );
}
