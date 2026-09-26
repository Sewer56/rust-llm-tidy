//! FMT001 member spacing over the C# fixture.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// FMT001 warns on documented members with no blank line between them.
#[test]
fn csharp_fmt001_warns_on_unseparated_documented_members() {
    let (stderr, exit) = run_csharp_fixture("fmt001_member_spacing.cs");

    assert_eq!(exit, 0, "FMT001 warnings must not fail the run");
    for name in ["Edge", "Exit"] {
        assert_has_diagnostic(&stderr, "FMT001", Some(name));
    }
    assert_eq!(
        stderr.matches("FMT001").count(),
        2,
        "expected exactly 2 FMT001 findings:\n{stderr}"
    );
}
