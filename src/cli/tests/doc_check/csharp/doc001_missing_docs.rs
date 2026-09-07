//! DOC001 missing doc comments over the C# fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// DOC001 flags every undocumented non-private member kind and skips
/// private, unmodified, and documented members.
#[test]
fn csharp_doc001_flags_undocumented_non_private_members() {
    let (stderr, exit) = run_csharp_fixture("doc001_missing_docs.cs");
    assert_ne!(exit, 0, "DOC001 errors must fail the run");

    for name in [
        "Undocumented",
        "Guarded",
        "Cached",
        "Shape",
        "Kind",
        "Notify",
        "Changed",
        "Alpha",
    ] {
        assert_has_diagnostic(&stderr, "DOC001", Some(name));
    }
    for clean in [
        "Hidden",
        "InternalDefault",
        "Documented",
        "IBehavior",
        "Apply",
    ] {
        assert!(
            !stderr.contains(&format!("`{clean}`")),
            "`{clean}` must not be flagged:\n{stderr}"
        );
    }
    assert_eq!(
        stderr.matches("DOC001").count(),
        8,
        "expected exactly 8 DOC001 findings:\n{stderr}"
    );
}
