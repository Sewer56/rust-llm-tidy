//! DOC011 missing `<returns>` tags over the C# fixtures.
//!
//! Runs `--include DOC011` on a fixture in `tests/fixtures/doc/csharp/`
//! and asserts on its exit code and stderr diagnostics.

use super::csharp_fixture_dir;
use crate::{assert_has_diagnostic, run_command};

/// DOC011 warns on the value-returning method and reminds on the
/// `bool`-returning one; everything else passes.
#[test]
fn csharp_doc011_warns_on_missing_returns_tags() {
    let path = csharp_fixture_dir().join("doc011_missing_returns.cs");
    let output = run_command(&["--include", "DOC011"], &path);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit = output.status.code().unwrap_or(-1);

    assert_eq!(exit, 0, "DOC011 findings must not fail the run:\n{stderr}");
    assert!(
        stderr.contains("warning[DOC011]") && stderr.contains("GetCount"),
        "the value-returning method must warn:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[DOC011]") && stderr.contains("IsReady"),
        "the bool-returning method must remind:\n{stderr}"
    );
    assert_has_diagnostic(&stderr, "DOC011", Some("GetCount"));
    assert!(
        !stderr.contains("Tagged") && !stderr.contains("Reset") && !stderr.contains("Hidden"),
        "tagged, void, and private members pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC011").count(),
        2,
        "expected exactly 2 DOC011 findings:\n{stderr}"
    );
}
