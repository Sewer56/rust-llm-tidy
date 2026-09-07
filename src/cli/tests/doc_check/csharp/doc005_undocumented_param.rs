//! DOC005 undocumented parameters over the C# fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// DOC005 names the parameter the `<param>` tags omitted.
#[test]
fn csharp_doc005_names_the_undocumented_param() {
    let (stderr, exit) = run_csharp_fixture("doc005_undocumented_param.cs");

    assert_eq!(exit, 0, "DOC005 warnings must not fail the run");
    assert_has_diagnostic(&stderr, "DOC005", Some("Build"));
    assert!(
        stderr.contains("`format`"),
        "DOC005 must name the omitted parameter:\n{stderr}"
    );
    assert!(
        !stderr.contains("`Built`"),
        "fully documented members pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC005").count(),
        1,
        "expected exactly 1 DOC005 finding:\n{stderr}"
    );
}
