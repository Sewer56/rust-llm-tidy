//! Integration tests for the `check` and `all` subcommands of
//! `rust-llm-tidy`.
//!
//! These are kept separate from the reorder integration tests
//! (`integration.rs`) so the documentation-lint behavior is exercised in
//! isolation.
//!
//! Each test runs the built CLI binary against a fixture or temp file and
//! asserts on its exit code and stderr diagnostics.

use common::binary;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

mod csharp;
mod rust;
// The folder root sits inside `tests/doc_check/`, so the helpers shared by
// every test binary resolve at their sibling path, not under this folder.
#[path = "../common/mod.rs"]
mod common;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// ── `all` subcommand ──────────────────────────────────────────────

/// `all` on a clean file passes with no diagnostics.
#[test]
fn all_clean_file() {
    let path = rust_fixture_dir().join("clean.rs");
    let output = run_command(&["--dry-run"], &path);

    assert!(
        output.status.success(),
        "all on clean file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `all --dry-run` applies table fixes to a `.md` file and reports the change
/// record on stderr, leaving stdout empty.
#[test]
fn all_md_dry_run_fixes_tables() {
    let before = fix_fixture_dir().join("table_md_before.md");
    let output = run_command(&["--dry-run"], &before);

    assert!(
        output.status.success(),
        "all --dry-run on markdown should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        "all --dry-run must report the table fix on stderr: {stderr}"
    );
}

/// `all` fixes markdown tables in place but skips reorder/check for `.md`.
#[test]
fn all_md_in_place_fixes_tables() {
    let expected = fs::read_to_string(fix_fixture_dir().join("table_md_after.md")).unwrap();
    let tmp = temp_file("md");
    fs::write(
        &tmp,
        fs::read_to_string(fix_fixture_dir().join("table_md_before.md")).unwrap(),
    )
    .unwrap();

    let output = run_command(&[], &tmp);
    assert!(
        output.status.success(),
        "all on markdown file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(actual, expected, "in-place markdown fix must match after");
}

// ── Error handling ────────────────────────────────────────────────

/// `all` on a file with doc gaps reports them after reordering.
#[test]
fn all_reports_remaining_doc_gaps() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("gap.rs");
    std::fs::write(&file, "pub fn undocumented() {}\n").unwrap();

    let output = run_command(&[], &file);

    assert!(
        !output.status.success(),
        "all should fail on remaining doc gaps"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DOC001"),
        "all should report doc gaps, got:\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Assert that `stderr` contains the given diagnostic code and optional item
/// name.
fn assert_has_diagnostic(stderr: &str, code: &str, item_name: Option<&str>) {
    assert!(
        stderr.contains(code),
        "stderr should contain {code}, got:\n{stderr}"
    );
    if let Some(name) = item_name {
        assert!(
            stderr.contains(name),
            "stderr should mention `{name}`, got:\n{stderr}"
        );
    }
}

/// A non-existent path is rejected.
#[test]
fn check_nonexistent_path_fails() {
    let nonexistent = std::env::temp_dir().join(format!(
        "rust-llm-tidy-lint-missing-{}-{}.rs",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let output = run_command(&["--include", "lints"], &nonexistent);

    assert!(
        !output.status.success(),
        "non-existent path should exit non-zero"
    );
}

// ── Directory recursion ───────────────────────────────────────────

/// `check` descends into directories recursively.
#[test]
fn check_recursive_directory() {
    let dir = temp_dir();
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    // Clean file at the root.
    std::fs::copy(rust_fixture_dir().join("clean.rs"), dir.join("clean.rs")).unwrap();
    // Undocumented file in a nested dir.
    std::fs::write(sub.join("dirty.rs"), "pub fn dirty() {}\n").unwrap();

    let output = run_command(&["--include", "lints"], &dir);

    assert!(
        !output.status.success(),
        "directory with missing docs should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DOC001") && stderr.contains("dirty.rs"),
        "should flag the nested undocumented file, got:\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Default-run text checks: every comment-marker family ─────────

/// A default run (no rule selection) over a mixed-language fixture
/// tree reports the over-budget comment paragraph in every
/// comment-marker family at its original line; the string content in
/// each fixture stays unmeasured.
#[test]
fn default_run_lints_comment_prose_in_every_comment_family() {
    let names = [
        "default_budgets.go",
        "default_budgets.rb",
        "default_budgets.sql",
        "default_budgets.el",
        "default_budgets.erl",
    ];
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for name in names {
        fs::copy(defaults_fixture_dir().join(name), dir.join(name)).unwrap();
    }

    let output = run_command(&["--dry-run"], &dir);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the TEXT001 errors must fail the default run:\n{stderr}"
    );
    for name in names {
        assert!(
            stderr.contains(&format!("{name}:1: error[TEXT001]")),
            "{name}: the comment paragraph must fire in the default run:\n{stderr}"
        );
    }
    assert_eq!(
        stderr.matches("TEXT001").count(),
        names.len(),
        "exactly one comment paragraph per family, never the string content:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "no doc line in the fixtures crosses the line budget:\n{stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// ── Lexicon text checks: comment-marker families ──────────────────

/// Lisp `;` and `#| |#` comment prose fires the text budgets at
/// original file lines while `"..."` string content stays quiet.
#[test]
fn el_lexicon_measures_comments_not_strings() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.el");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":9: error[TEXT001]"),
        "TEXT001 must report at the block comment's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":15: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "exactly the line and block comment paragraphs, never the string:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "exactly the over-long comment line, never the string:\n{stderr}"
    );
}

/// Erlang `%` comment prose fires the text budgets while `<<"...">>`
/// binary content stays quiet.
#[test]
fn erl_lexicon_measures_comments_not_strings() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.erl");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":9: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "exactly the comment paragraph, never the binary literal:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "exactly the over-long comment line, never the binary literal:\n{stderr}"
    );
}

/// JS template literal and string content produce no findings on a
/// probe file whose mis-measured lines would overflow both budgets.
#[test]
fn js_lexicon_string_probes_stay_quiet() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_probes.js");

    assert_eq!(exit, 0, "the probe fixture must be clean:\n{stderr}");
    assert!(
        stderr.is_empty(),
        "template literal and string content must stay unmeasured:\n{stderr}"
    );
}

/// Explicit `--include lints` on a `.js` file emits TEXT001 for
/// over-budget `//` and `/** */` prose and TEXT002 for an over-long
/// comment line, all at original file lines.
#[test]
fn js_lexicon_text_budgets_fire_with_original_lines() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.js");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":11: error[TEXT001]"),
        "TEXT001 must report at the JSDoc paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":20: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "expected exactly 2 TEXT001 findings:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "expected exactly 1 TEXT002 finding:\n{stderr}"
    );
}

// ── JSON output mode ──────────────────────────────────────────────

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
        alias.status.success(),
        "--json dry-run should succeed: {}",
        String::from_utf8_lossy(&alias.stderr)
    );
    assert!(
        mode.status.success(),
        "--output-mode json dry-run should succeed: {}",
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

/// `--output-mode json --dry-run` succeeds (no clap conflict) on a clean file
/// and emits parseable JSON.
#[test]
fn json_output_combines_with_dry_run() {
    let path = rust_fixture_dir().join("clean.rs");
    let output = run_command(&["--output-mode", "json", "--dry-run"], &path);

    assert!(
        output.status.success(),
        "JSON output must combine with --dry-run: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}\n{stdout}"));
}

/// One `--output-mode json --dry-run` document carries every lint finding and
/// every recorded change together, even when error-severity lints bail the run
/// non-zero after the document is written.
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

/// `--output-mode json --dry-run` records the would-be reorder as a
/// `severity: "success"` record carrying its move positions on stdout, with no
/// JSON duplicated on stderr.
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
        output.status.success(),
        "reorder dry-run in JSON mode should succeed: {}",
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
        let keys: std::collections::BTreeSet<&str> = finding
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

// ── Markdown lint dispatch ────────────────────────────────────────

/// A clean markdown file passes lint dispatch with no diagnostics.
#[test]
fn md_clean_file_no_diagnostics() {
    let path = temp_md("# Title\n\nShort prose paragraph.\n");
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "clean markdown should pass lint dispatch: {stderr}"
    );
    assert!(
        stderr.is_empty(),
        "clean markdown should produce no diagnostics, got:\n{stderr}"
    );
}

/// A whitelist without `lints` (or any lint code) skips linting entirely,
/// including the markdown text checks.
#[test]
fn md_lints_skipped_when_whitelist_omits_lints() {
    let path = temp_md(&oversized_paragraph_md());
    let output = run_command(&["--include", "tables", "--dry-run"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "tables-only whitelist must not lint markdown: {stderr}"
    );
    assert!(
        !stderr.contains("TEXT001") && !stderr.contains("TEXT002"),
        "lint findings must be suppressed without `lints` in the whitelist:\n{stderr}"
    );
}

/// A markdown line over 80 chars yields a TEXT002 warning without failing.
#[test]
fn md_long_line_warns_text002_without_failing() {
    let path = temp_md(&format!("{}\n", "x".repeat(81)));
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT002 warnings must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "expected a TEXT002 warning at line 1, got:\n{stderr}"
    );
}

/// An over-limit markdown paragraph fails the run with a TEXT001 error; the
/// file is no longer skipped before linting.
#[test]
fn md_paragraph_over_limit_fails_with_text001() {
    let path = temp_md(&oversized_paragraph_md());
    let output = run_command(&["--include", "lints"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "TEXT001 errors on markdown must fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":3: error[TEXT001]"),
        "expected a TEXT001 error at the paragraph's first line, got:\n{stderr}"
    );
}

/// `--exclude TEXT001` suppresses the markdown paragraph error; the run then
/// succeeds with no findings.
#[test]
fn md_text001_suppressed_by_exclude() {
    let path = temp_md(&oversized_paragraph_md());
    let output = run_command(&["--include", "lints", "--exclude", "TEXT001"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "excluding TEXT001 must clear the markdown error: {stderr}"
    );
    assert!(
        !stderr.contains("TEXT001"),
        "excluded code must not be reported, got:\n{stderr}"
    );
}

/// Python docstring prose fires the text budgets with original file
/// lines: TEXT001 errors on the module docstring's over-budget paragraph
/// and TEXT002 warns on a function docstring's over-long line.
///
/// The non-docstring triple-quoted payload and the `>>>` doctest example
/// stay quiet.
#[test]
fn py_docstring_budgets_fire_with_original_lines() {
    let (stderr, exit) = run_python_fixture("docstring_text_budgets.py");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":2: error[TEXT001]"),
        "TEXT001 must report at the docstring's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":27: warning[TEXT002]"),
        "TEXT002 must report at the over-long docstring line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "the docstring paragraph only, never the payload:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "the wide docstring line only, never the doctest:\n{stderr}"
    );
}

/// Python `#` comment prose fires TEXT001 while triple-quoted string
/// content and `<<` operators stay quiet.
#[test]
fn py_text_checks_measure_comments_not_strings() {
    let (stderr, exit) = run_python_fixture("doc_text_lexicon_budgets.py");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "exactly the comment paragraph, never the string content:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "no doc line in the fixture crosses the line budget:\n{stderr}"
    );
}

/// Ruby `#` comments measure as prose - heading-shaped `##` lines
/// included - while `<<` operators, heredoc payload, and code lines
/// stay quiet.
#[test]
fn rb_lexicon_measures_comment_prose_only() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.rb");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "the comment paragraph must measure from its first line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "exactly the comment paragraph, never payload or code:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "no doc line in the fixture crosses the line budget:\n{stderr}"
    );
}

/// Run `rust-llm-tidy --include lints` on a Rust fixture and return its
/// (stderr, exit_code).
fn run_rust_fixture(name: &str) -> (String, i32) {
    let path = rust_fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Shell heredoc payload and `$#` parameter syntax stay quiet while the
/// comment paragraph fires.
#[test]
fn sh_lexicon_ignores_heredoc_payload() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.sh");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "exactly the comment paragraph, never the heredoc payload:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "no doc line in the fixture crosses the line budget:\n{stderr}"
    );
}

/// SQL `--` and `/* */` comment prose fires the text budgets at
/// original file lines while `'...'` string content stays quiet.
#[test]
fn sql_lexicon_measures_comments_not_strings() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.sql");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":8: error[TEXT001]"),
        "TEXT001 must report at the block comment's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":13: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "exactly the line and block comment paragraphs, never the string:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "exactly the over-long comment line, never the string:\n{stderr}"
    );
}

// ── Helpers ───────────────────────────────────────────────────────

/// The directory holding the default-run mixed-language fixtures.
fn defaults_fixture_dir() -> std::path::PathBuf {
    fixture_dir().join("defaults")
}

/// The directory holding fix fixtures.
fn fix_fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("fix")
}

/// A markdown paragraph over 240 chars built from short (under-80) lines, so
/// only TEXT001 fires on it.
fn oversized_paragraph_md() -> String {
    let lines: String = (0..10)
        .map(|i| format!("sentence number {i} carries some filler text\n"))
        .collect();
    format!("# Title\n\n{lines}\nTrailer.\n")
}

/// The reorder fixture root; callers join the language dir (`rust` or
/// `csharp`) before the fixture name.
fn reorder_fixture_dir() -> std::path::PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder")
}

/// Run `rust-llm-tidy --include lints` on a lexicon-family fixture and
/// return its (stderr, exit_code).
fn run_lexicon_fixture(name: &str) -> (String, i32) {
    let path = fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Run `rust-llm-tidy --include lints` on a Python fixture and return its
/// (stderr, exit_code).
fn run_python_fixture(name: &str) -> (String, i32) {
    let path = python_fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// The directory holding the Rust lint fixtures.
fn rust_fixture_dir() -> std::path::PathBuf {
    fixture_dir().join("rust")
}

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-lint-dir-{}-{}", pid, seq))
}

/// Writes `content` to a numbered temp `.md` file and returns its path.
fn temp_md(content: &str) -> std::path::PathBuf {
    let path = temp_file("md");
    fs::write(&path, content).unwrap();
    path
}

/// The directory holding the Python lint fixtures.
fn python_fixture_dir() -> std::path::PathBuf {
    fixture_dir().join("python")
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(["--no-config"]).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Create a numbered temporary file path with the given extension.
fn temp_file(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-all-{}-{}.{}", pid, seq, ext))
}

/// The directory holding the shared, cross-language lint fixtures.
fn fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("doc")
}

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
