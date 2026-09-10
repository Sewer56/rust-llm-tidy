//! TEST002 test-summary comments over the Rust fixtures.
//!
//! Runs `--include TEST002` on `test002_test_summary.rs` and asserts its
//! finding count, exit code, and JSON record fields. The shared runner
//! helpers live in `mod.rs` and the crate root.

use super::run_rust_fixture;
use crate::{run_command, rust_fixture_dir, temp_named_file};

/// TEST002 reaches the same decision in both backends for the same comment
/// adjacency.
///
/// Runs the real binary on a Rust and a C# source for an adjacent and a
/// detached doc comment, then compares the normalized JSON records. Only the
/// language noun and the source syntax differ.
#[test]
fn test002_decisions_match_across_backends() {
    let cases = [
        (
            "adjacent",
            "/// Verifies the loader.\n#[test]\nfn loads() {}\n",
            "class C\n{\n    /// <summary>Verifies the loader.</summary>\n    [TestMethod]\n    public void loads() { }\n}\n",
        ),
        (
            "detached",
            "/// Verifies the loader.\n\n#[test]\nfn loads() {}\n",
            "class C\n{\n    /// <summary>Verifies the loader.</summary>\n\n    [TestMethod]\n    public void loads() { }\n}\n",
        ),
    ];

    for (name, rust, csharp) in cases {
        let rust_records = test002_records(&temp_named_file(&format!("{name}.rs"), rust));
        let csharp_records = test002_records(&temp_named_file(&format!("{name}.cs"), csharp));

        assert_eq!(
            rust_records, csharp_records,
            "{name}: TEST002 must decide identically across backends"
        );
    }
}

/// Exactly the four unsummarised test functions produce findings, and the
/// commented ones do not.
#[test]
fn test002_flags_only_unsummarised_tests() {
    let (stderr, exit) = run_rust_fixture("test002_test_summary.rs", "TEST002");

    // TEST002 is error-severity, so the run fails.
    assert_ne!(exit, 0, "TEST002 findings should fail the run");

    for name in [
        "missing_summary",
        "blank_line_before_attributes",
        "comment_below_attributes",
        "ignored_test_without_summary",
    ] {
        assert!(
            stderr
                .lines()
                .any(|l| l.contains("TEST002") && l.contains(name)),
            "expected TEST002 on `{name}`:\n{stderr}"
        );
    }
    for name in ["doc_comment_summary", "plain_comment_summary", "not_a_test"] {
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

/// The JSON record carries the documented fields and message sections.
#[test]
fn test002_json_record_carries_the_documented_fields() {
    let path = rust_fixture_dir().join("test002_test_summary.rs");
    let output = run_command(&["--include", "TEST002", "--output-mode", "json"], &path);

    assert!(
        !output.status.success(),
        "error-severity findings fail the run"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = findings.as_array().expect("output must be an array");
    assert_eq!(array.len(), 4, "one record per finding:\n{stdout}");

    for rec in array {
        assert_eq!(rec["code"], "TEST002");
        assert_eq!(rec["severity"], "error");
        assert_eq!(rec["title"], "test missing its summary comment");
        assert_eq!(rec["item_kind"], "fn");
        assert!(rec["line"].as_u64().is_some_and(|l| l >= 1));

        let message = rec["message"].as_str().expect("message is a string");
        assert!(
            message.starts_with("test function `")
                && message.contains("is missing a short explanatory comment above its attributes.")
                && message.contains("Why:")
                && message.contains("Suggestions:")
                && message.contains("separated by comments."),
            "unexpected message:\n{message}"
        );
    }
}

/// Run TEST002 in JSON mode and return each finding's `code/severity` plus its
/// message with the language noun normalized away.
fn test002_records(path: &std::path::Path) -> Vec<(String, String)> {
    let output = run_command(&["--include", "TEST002", "--output-mode", "json"], path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));

    findings
        .as_array()
        .expect("output must be an array")
        .iter()
        .map(|rec| {
            (
                format!("{}/{}", rec["code"], rec["severity"]),
                rec["message"]
                    .as_str()
                    .expect("message")
                    .replace("test method", "test function"),
            )
        })
        .collect()
}
