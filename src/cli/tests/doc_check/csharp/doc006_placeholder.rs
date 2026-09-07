//! DOC006 doc-comment placeholders over the C# fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// DOC006 warns on TODO/FIXME/TBD placeholder markers in C# doc comments.
#[test]
fn csharp_doc006_warns_on_placeholders() {
    let (stderr, exit) = run_csharp_fixture("doc006_placeholders.cs");

    assert_eq!(exit, 0, "DOC006 warnings must not fail the run");
    for name in ["Todo", "Fixme", "Tbd"] {
        assert_has_diagnostic(&stderr, "DOC006", Some(name));
    }
    assert!(
        !stderr.contains("`Done`"),
        "described members pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC006").count(),
        3,
        "expected exactly 3 DOC006 findings:\n{stderr}"
    );
}
