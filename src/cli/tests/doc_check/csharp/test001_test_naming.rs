//! TEST001 discouraged test-method names over the C# fixtures.
//!
//! Every test runs `--include lints` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_csharp_fixture;
use crate::assert_has_diagnostic;

/// TEST001 flags marker-attributed methods with discouraged names and
/// passes the behavioral name.
#[test]
fn csharp_test001_flags_discouraged_names() {
    let (stderr, _exit) = run_csharp_fixture("test001_test_naming.cs");

    for name in ["Test1", "Test_foo", "Case_1", "Test"] {
        assert_has_diagnostic(&stderr, "TEST001", Some(name));
    }
    assert!(
        !stderr
            .lines()
            .any(|l| l.contains("TEST001") && l.contains("ShouldReturnZeroWhenEmpty")),
        "the behavioral name passes:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEST001").count(),
        4,
        "expected exactly 4 TEST001 findings:\n{stderr}"
    );
}
