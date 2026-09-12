//! DOC010 XML doc tag order over the C# fixture.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// DOC010 errors on the member whose tags decrease in canonical order.
#[test]
fn csharp_doc010_errors_on_tags_out_of_canonical_order() {
    let (stderr, exit) = run_csharp_fixture("doc010_tag_order.cs");

    assert_ne!(exit, 0, "DOC010 errors must fail the run");
    assert!(stderr.contains("error[DOC010]"), "error framing:\n{stderr}");
    assert_has_diagnostic(&stderr, "DOC010", Some("Save"));
    assert!(
        !stderr.contains("`Load`"),
        "canonical-order member passes:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC010").count(),
        1,
        "expected exactly 1 DOC010 finding:\n{stderr}"
    );
}
