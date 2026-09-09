//! `--output-mode json` record contracts of the lint and fix runs.
//!
//! Every test runs the built CLI binary and parses the JSON document on
//! stdout: field sets, severities, and the lint/change record split. The
//! shared runner helpers live in `mod.rs`.

use crate::csharp::csharp_fixture_dir;
use crate::{fix_fixture_dir, reorder_fixture_dir, run_command, rust_fixture_dir, temp_file};
use std::collections::BTreeSet;
use std::fs;

/// JSON dry-run records the would-be using hoist and member reorder.
///
/// Runs `--include reorder --output-mode json --dry-run` on a `.cs` file.
/// Both records use `severity: "success"` and no title, exactly like the
/// Rust reorder records.
#[test]
fn csharp_json_dry_run_records_the_member_reorder() {
    let path = reorder_fixture_dir()
        .join("csharp")
        .join("reorder_cs_before.cs");
    let output = run_command(
        &["--include", "reorder", "--output-mode", "json", "--dry-run"],
        &path,
    );

    assert!(
        !output.status.success(),
        "JSON dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = records.as_array().expect("output must be an array");
    assert_eq!(
        array.len(),
        2,
        "expected the using hoist and the member reorder:\n{stdout}"
    );
    for rec in array {
        assert_eq!(rec["severity"], "success");
        assert_eq!(rec["code"], "REORDER");
        assert!(
            rec["title"].is_null(),
            "change records carry no title:\n{stdout}"
        );
    }
    assert!(
        array
            .iter()
            .any(|r| r["item_kind"] == "class" && r["item_name"] == "OrderService"),
        "one record names the reordered class:\n{stdout}"
    );
    assert!(
        array.iter().any(|r| r["item_kind"] == "using"),
        "one record names the hoisted using:\n{stdout}"
    );
}

/// `--output-mode json` on a `.cs` file emits the documented lint record
/// shape: the same field set as Rust findings, with the friendly title.
#[test]
fn csharp_json_output_matches_the_documented_record_shape() {
    let path = csharp_fixture_dir().join("doc004_missing_param.cs");
    let output = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert!(
        output.status.success(),
        "warnings-only JSON run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = findings.as_array().expect("output must be an array");
    assert_eq!(
        array.len(),
        1,
        "expected exactly the DOC004 finding:\n{stdout}"
    );

    let keys: BTreeSet<&str> = array[0]
        .as_object()
        .expect("the finding is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "path",
            "line",
            "severity",
            "code",
            "message",
            "item_kind",
            "item_name",
            "title",
        ]
        .into_iter()
        .collect(),
        "C# findings carry exactly the documented fields: {stdout}"
    );
    assert_eq!(array[0]["severity"], "warning");
    assert_eq!(array[0]["code"], "DOC004");
    assert_eq!(array[0]["title"], "missing `# Arguments` section");
    assert_eq!(array[0]["item_kind"], "fn");
    assert_eq!(array[0]["item_name"], "Greet");
    assert!(array[0]["line"].as_u64().is_some_and(|l| l >= 1));
}

/// The `--json` alias is equivalent to `--output-mode json`.
#[test]
fn json_alias_is_equivalent_to_output_mode() {
    let path = rust_fixture_dir().join("clean.rs");
    let alias = run_command(&["--include", "lints", "--json"], &path);
    let mode = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert_eq!(alias.status.code(), mode.status.code());
    assert_eq!(alias.stdout, mode.stdout);
}

/// `--json --dry-run` and `--output-mode json --dry-run` are equivalent and
/// both record the would-be reorder with `severity: "success"`.
#[test]
fn json_dry_run_records_changes_for_both_flags() {
    let path = reorder_fixture_dir()
        .join("rust")
        .join("fn_interstitial_comment_travels_with_next_before.rs");
    let alias = run_command(&["--include", "reorder", "--json", "--dry-run"], &path);
    let mode = run_command(
        &["--include", "reorder", "--output-mode", "json", "--dry-run"],
        &path,
    );

    assert!(
        !alias.status.success(),
        "--json dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&alias.stderr)
    );
    assert!(
        !mode.status.success(),
        "--output-mode json dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&mode.stderr)
    );
    assert_eq!(
        alias.stdout, mode.stdout,
        "--json and --output-mode json must be equivalent in dry-run"
    );

    for output in [&alias, &mode] {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let records: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
        let array = records.as_array().expect("JSON output must be an array");
        assert_eq!(array.len(), 1, "expected 1 reorder record, got:\n{stdout}");
        assert_eq!(array[0]["severity"], "success");
        assert_eq!(array[0]["code"], "REORDER");
        assert!(
            array[0]["title"].is_null(),
            "change records carry no title, got:\n{stdout}"
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.trim().starts_with('['),
            "stderr must not carry JSON, got:\n{stderr}"
        );
    }
}

/// A non-dry-run fix in JSON mode reports each edited table as a `success`
/// record on stdout and writes the file in place.
#[test]
fn json_in_place_run_records_fix_changes() {
    let expected = fs::read_to_string(fix_fixture_dir().join("table_md_after.md")).unwrap();
    let tmp = temp_file("md");
    fs::write(
        &tmp,
        fs::read_to_string(fix_fixture_dir().join("table_md_before.md")).unwrap(),
    )
    .unwrap();

    let output = run_command(&["--include", "tables", "--output-mode", "json"], &tmp);

    assert!(
        output.status.success(),
        "fix in-place in JSON mode should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, expected,
        "in-place fix must write the table_md_after fixture"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = records.as_array().expect("JSON output must be an array");
    assert_eq!(array.len(), 1, "expected 1 fix record, got:\n{stdout}");
    let rec = &array[0];
    assert_eq!(rec["severity"], "success");
    assert_eq!(rec["code"], "FIX");
    assert_eq!(rec["item_kind"], "table");
    assert!(rec["line"].is_null(), "table records carry no line");
    assert!(rec["title"].is_null(), "change records carry no title");
    assert_eq!(rec["message"], "tables were aligned");
}

/// A non-dry-run reorder in JSON mode reports `success` records in the same
/// document on stdout and writes the file in place.
#[test]
fn json_in_place_run_records_reorder_changes() {
    let expected = fs::read_to_string(
        reorder_fixture_dir()
            .join("rust")
            .join("fn_interstitial_comment_travels_with_next_after.rs"),
    )
    .unwrap();
    let tmp = temp_file("rs");
    fs::write(
        &tmp,
        fs::read_to_string(
            reorder_fixture_dir()
                .join("rust")
                .join("fn_interstitial_comment_travels_with_next_before.rs"),
        )
        .unwrap(),
    )
    .unwrap();

    let output = run_command(&["--include", "reorder", "--output-mode", "json"], &tmp);

    assert!(
        output.status.success(),
        "reorder in-place in JSON mode should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, expected,
        "in-place reorder must write the after fixture"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = records.as_array().expect("JSON output must be an array");
    assert_eq!(array.len(), 1, "expected 1 reorder record, got:\n{stdout}");
    let rec = &array[0];
    assert_eq!(rec["severity"], "success");
    assert_eq!(rec["code"], "REORDER");
}

/// `--output-mode json` on a clean file prints `[]` and exits 0.
#[test]
fn json_output_clean_file_prints_empty_array() {
    let path = rust_fixture_dir().join("clean.rs");
    let output = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[]\n");
}

/// JSON preview emits parseable output when a lint-clean file needs reordering.
#[test]
fn json_output_combines_with_dry_run() {
    let path = rust_fixture_dir().join("clean.rs");
    let output = run_command(&["--output-mode", "json", "--dry-run"], &path);

    assert!(
        !output.status.success(),
        "JSON dry-run must fail for proposed reordering: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
}

/// One `--output-mode json --dry-run` document carries every lint
/// finding and every recorded change together.
///
/// Error-severity lints still bail the run non-zero after the document
/// is written.
#[test]
fn json_output_merges_lints_and_changes_in_one_document() {
    let path = rust_fixture_dir().join("doc001_missing_docs.rs");
    let output = run_command(
        &[
            "--include",
            "lints",
            "--include",
            "reorder",
            "--output-mode",
            "json",
            "--dry-run",
        ],
        &path,
    );

    assert!(
        !output.status.success(),
        "DOC001 error findings must still fail a dry-run JSON run"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("one JSON doc must parse despite the error bail: {e}\n{stdout}")
    });
    let array = records.as_array().expect("JSON output must be an array");
    assert!(
        array.iter().any(|r| r["severity"] == "error"
            && r["code"] == "DOC001"
            && r["title"] == "missing documentation"),
        "array must contain titled lint findings, got:\n{stdout}"
    );
    assert!(
        array
            .iter()
            .any(|r| r["severity"] == "success" && r["code"] == "REORDER" && r["title"].is_null()),
        "array must contain untitled reorder change records, got:\n{stdout}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().starts_with('['),
        "stderr must not carry JSON, got:\n{stderr}"
    );
}

/// `--output-mode json` on an error-severity fixture exits non-zero and still
/// prints the JSON document on stdout (before the error-count bail).
#[test]
fn json_output_prints_document_before_error_bail() {
    let path = rust_fixture_dir().join("doc001_missing_docs.rs");
    let output = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert!(
        !output.status.success(),
        "error-severity findings must fail the run in JSON mode"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON despite error bail: {e}\n{stdout}"));
    let array = findings.as_array().expect("JSON output must be an array");
    assert!(
        !array.is_empty(),
        "error fixture must produce findings on stdout"
    );
    assert!(array.iter().any(|f| f["severity"] == "error"));
    assert!(
        array
            .iter()
            .filter(|f| f["code"] == "DOC001")
            .all(|f| f["title"] == "missing documentation"),
        "DOC001 findings must carry the friendly title, got:\n{stdout}"
    );
}

/// `--output-mode json --dry-run` records the would-be reorder on stdout.
///
/// The record carries `severity: "success"` with its move positions, and no
/// JSON is duplicated on stderr.
#[test]
fn json_output_records_reorder_changes() {
    let path = reorder_fixture_dir()
        .join("rust")
        .join("fn_interstitial_comment_travels_with_next_before.rs");
    let output = run_command(
        &["--include", "reorder", "--output-mode", "json", "--dry-run"],
        &path,
    );

    assert!(
        !output.status.success(),
        "reorder dry-run in JSON mode must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = records.as_array().expect("JSON output must be an array");
    assert_eq!(array.len(), 1, "expected 1 reorder record, got:\n{stdout}");
    let rec = &array[0];
    assert_eq!(rec["severity"], "success");
    assert_eq!(rec["code"], "REORDER");
    // Presence and null are separate pins: indexing a missing key also
    // yields null, so `is_null` alone would not catch a dropped field.
    assert!(
        rec.as_object()
            .expect("change record is an object")
            .contains_key("title"),
        "change records must emit an explicit title: null key, got:\n{stdout}"
    );
    assert!(
        rec["title"].is_null(),
        "change records carry no title, got:\n{stdout}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().starts_with('['),
        "stderr must not carry JSON, got:\n{stderr}"
    );
}

/// `--output-mode json` prints one JSON array on stdout with every finding
/// for all processed files, using the documented fields and lowercase severity.
#[test]
fn json_output_reports_all_findings() {
    let path = rust_fixture_dir().join("test001_test_naming.rs");
    let output = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert!(
        output.status.success(),
        "warnings-only JSON run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
    let array = findings.as_array().expect("JSON output must be an array");

    assert_eq!(
        array.len(),
        3,
        "expected 3 TEST001 findings, got:\n{stdout}"
    );
    for finding in array {
        // Pin the exact field set: lint records carry only the base fields.
        let keys: BTreeSet<&str> = finding
            .as_object()
            .expect("each record is an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "path",
                "line",
                "severity",
                "code",
                "message",
                "item_kind",
                "item_name",
                "title"
            ]
            .into_iter()
            .collect(),
            "lint record must not carry change-only extras, got: {finding}"
        );
        assert_eq!(finding["severity"], "warning");
        assert_eq!(finding["code"], "TEST001");
        assert_eq!(finding["title"], "non-behavioral test name");
        assert_eq!(finding["item_kind"], "fn");
        assert_eq!(
            finding["path"],
            path.to_str().unwrap(),
            "finding must carry the processed file path"
        );
        assert!(finding["line"].as_u64().is_some_and(|l| l >= 1));
        assert!(
            finding["message"].as_str().is_some_and(|m| !m.is_empty()),
            "message field must be present and non-empty, got: {finding}"
        );
        assert!(
            finding["item_name"].is_string(),
            "item_name must be a string for a named item, got: {finding}"
        );
    }

    // Pin the null-when-absent contract: item_name is null only for
    // unnamed items, and no finding in this fixture is unnamed.
    assert!(
        array.iter().all(|f| f["item_name"].is_string()),
        "item_name must be non-null for named test functions, got:\n{stdout}"
    );

    // No plaintext diagnostic line should reach stderr in JSON mode.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("TEST001"),
        "JSON mode must not duplicate diagnostics on stderr, got:\n{stderr}"
    );
}
