//! TEST002 test-summary comments over the C# fixtures.
//!
//! Runs `--include TEST002` on `test002_test_summary.cs` and asserts its
//! finding count and JSON record fields. The shared fixture-dir helper lives
//! in `mod.rs`; the binary runner in the crate root.

use super::csharp_fixture_dir;
use crate::run_command;

/// Exactly the four unsummarised test methods produce findings, and the
/// commented ones do not.
#[test]
fn test002_flags_only_unsummarised_tests() {
    let path = csharp_fixture_dir().join("test002_test_summary.cs");
    let output = run_command(&["--include", "TEST002"], &path);

    // TEST002 is reminder-severity, so the run exits 0.
    assert!(
        output.status.success(),
        "TEST002 reminders must not fail the run"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    for name in [
        "MissingSummary",
        "BlankLineBeforeAttributes",
        "CommentBelowAttributes",
        "IgnoredWithoutSummary",
    ] {
        assert!(
            stderr
                .lines()
                .any(|l| l.contains("TEST002") && l.contains(name)),
            "expected TEST002 on `{name}`:\n{stderr}"
        );
    }
    for name in ["DocCommentSummary", "PlainCommentSummary", "NotATest"] {
        assert!(
            !stderr
                .lines()
                .any(|l| l.contains("TEST002") && l.contains(name)),
            "`{name}` carries a summary comment and must pass:\n{stderr}"
        );
    }

    assert_eq!(
        stderr.matches("TEST002").count(),
        4,
        "expected exactly 4 TEST002 findings:\n{stderr}"
    );
}

/// The JSON record carries the documented fields and message sections with
/// the C# noun.
#[test]
fn test002_json_record_carries_the_documented_fields() {
    let path = csharp_fixture_dir().join("test002_test_summary.cs");
    let output = run_command(&["--include", "TEST002", "--output-mode", "json"], &path);

    assert!(output.status.success(), "reminder-severity findings exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = findings.as_array().expect("output must be an array");
    assert_eq!(array.len(), 4, "one record per finding:\n{stdout}");

    for rec in array {
        assert_eq!(rec["code"], "TEST002");
        assert_eq!(rec["severity"], "reminder");
        assert_eq!(rec["title"], "test missing its summary comment");
        assert_eq!(rec["item_kind"], "fn");
        assert!(rec["line"].as_u64().is_some_and(|l| l >= 1));

        let message = rec["message"].as_str().expect("message is a string");
        assert!(
            message.starts_with("test method `")
                && message.contains("is missing a short explanatory comment above its attributes.")
                && message.contains("Why:")
                && message.contains("Suggestions:")
                && message.contains("separated by comments."),
            "unexpected message:\n{message}"
        );
    }
}
