//! DOC004 missing `<param>` tags over the C# fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// DOC004 warns on the parameterized member without `<param>` tags.
#[test]
fn csharp_doc004_warns_on_missing_param_tags() {
    let (stderr, exit) = run_csharp_fixture("doc004_missing_param.cs");

    assert_eq!(exit, 0, "DOC004 warnings must not fail the run");
    assert_has_diagnostic(&stderr, "DOC004", Some("Greet"));
    assert!(
        !stderr.contains("`Greeted`") && !stderr.contains("`NoArgs`"),
        "tagged and parameterless members pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC004").count(),
        1,
        "expected exactly 1 DOC004 finding:\n{stderr}"
    );
}
