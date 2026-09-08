//! DOC003 vague `<exception>` crefs over the C# fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// DOC003 warns when `<exception>` tags carry no concrete `cref`.
#[test]
fn csharp_doc003_warns_on_vague_exception_crefs() {
    let (stderr, exit) = run_csharp_fixture("doc003_vague_exception.cs");

    assert_eq!(exit, 0, "DOC003 warnings must not fail the run");
    assert_has_diagnostic(&stderr, "DOC003", Some("Vague"));
    assert!(
        !stderr
            .lines()
            .any(|line| line.contains("[DOC003]") && line.contains("`Concrete`")),
        "a concrete cref passes:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC003").count(),
        1,
        "expected exactly 1 DOC003 finding:\n{stderr}"
    );
}
