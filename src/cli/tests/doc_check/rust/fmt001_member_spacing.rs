//! FMT001 member spacing over the Rust fixture.
//!
//! The test runs `--include FMT001` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// FMT001 warns on documented members with no blank line between them.
#[test]
fn fmt001_warns_on_unseparated_documented_members() {
    let (stderr, exit) = run_rust_fixture("fmt001_member_spacing.rs", "FMT001");

    assert_eq!(exit, 0, "FMT001 warnings must not fail the run");
    for name in ["LocalEdge", "EdgeKind", "EdgeTable"] {
        assert_has_diagnostic(&stderr, "FMT001", Some(name));
    }
    assert_eq!(
        stderr.matches("FMT001").count(),
        3,
        "expected exactly 3 FMT001 findings:\n{stderr}"
    );
}
