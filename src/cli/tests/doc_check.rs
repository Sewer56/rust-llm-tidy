//! Integration tests for the `check` and `all` subcommands of
//! `rust-llm-tidy`.
//!
//! These are kept separate from the reorder integration tests
//! (`integration.rs`) so the documentation-lint behavior is exercised in
//! isolation. Each test runs the built CLI binary against a fixture file in
//! `tests/fixtures/doc/` and asserts on its exit code and stderr diagnostics.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `all` on a clean file passes with no diagnostics.
#[test]
fn all_clean_file() {
    let path = fixture_dir().join("clean.rs");
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

// ── Helpers ───────────────────────────────────────────────────────

/// `check` descends into directories recursively.
#[test]
fn check_recursive_directory() {
    let dir = temp_dir();
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    // Clean file at the root.
    std::fs::copy(fixture_dir().join("clean.rs"), dir.join("clean.rs")).unwrap();
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

// ── `all` subcommand ──────────────────────────────────────────────

/// `clean.rs` is fully documented and produces zero diagnostics.
#[test]
fn clean_file_no_diagnostics() {
    let (stderr, exit) = run_check_fixture("clean.rs");
    assert_eq!(exit, 0, "clean file should pass");
    assert!(
        stderr.is_empty(),
        "clean file should produce no diagnostics, got:\n{stderr}"
    );
}

// ── Directory recursion ───────────────────────────────────────────

/// The documented function in `doc001_missing_docs.rs` is not flagged.
#[test]
fn doc001_documented_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc001_missing_docs.rs");
    assert!(
        !stderr.contains("`documented`"),
        "documented functions should not be flagged:\n{stderr}"
    );
}

/// `doc001_missing_docs.rs` flags every undocumented non-private documentable
/// item, skips private items and `pub use`.
#[test]
fn doc001_missing_docs() {
    let (stderr, exit) = run_check_fixture("doc001_missing_docs.rs");
    assert_ne!(exit, 0, "missing docs should fail");

    // Every undocumented public item is flagged.
    assert_has_diagnostic(&stderr, "DOC001", Some("alpha"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Beta"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Gamma"));
    assert_has_diagnostic(&stderr, "DOC001", Some("DELTA"));
    assert_has_diagnostic(&stderr, "DOC001", Some("EPSILON"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Zeta"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Eta"));
    assert_has_diagnostic(&stderr, "DOC001", Some("Theta"));

    // Count DOC001 occurrences: 8 expected.
    let doc001_count = stderr.matches("DOC001").count();
    assert_eq!(
        doc001_count, 8,
        "expected exactly 8 DOC001 findings, got {doc001_count}:\n{stderr}"
    );
}

/// The private function in `doc001_missing_docs.rs` is not flagged.
#[test]
fn doc001_private_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc001_missing_docs.rs");
    assert!(
        !stderr.contains("`helper`"),
        "private functions should not be flagged:\n{stderr}"
    );
}

/// `pub use` is not a documentable kind and must not be flagged.
#[test]
fn doc001_pub_use_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc001_missing_docs.rs");
    // The diagnostic kind for a use item would be `(use)`; verify it never
    // appears. Using a bare "use" substring would false-match the file path.
    assert!(
        !stderr.contains("(use)"),
        "pub use should not be flagged:\n{stderr}"
    );
}

// ── DOC002: missing `# Errors` section ────────────────────────────

/// `save` in the fixture has a complete `# Errors` section and is not flagged.
#[test]
fn doc002_documented_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("save"),
        "fn with complete # Errors should not be flagged:\n{stderr}"
    );
}

/// `doc002_missing_errors_section.rs` flags pub fns returning Result without
/// a `# Errors` section.
#[test]
fn doc002_missing_errors_section() {
    let (stderr, exit) = run_check_fixture("doc002_missing_errors_section.rs");
    assert_ne!(exit, 0, "missing # Errors should fail");

    assert_has_diagnostic(&stderr, "DOC002", Some("load"));
    assert_has_diagnostic(&stderr, "DOC002", Some("fetch"));

    // Exactly 2 DOC002 findings (load and fetch).
    let doc002_count = stderr.matches("DOC002").count();
    assert_eq!(
        doc002_count, 2,
        "expected exactly 2 DOC002 findings, got {doc002_count}:\n{stderr}"
    );
}

/// `count` returns a non-Result type and is not flagged.
#[test]
fn doc002_non_result_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("count"),
        "non-Result pub fns should not be flagged:\n{stderr}"
    );
}

// ── DOC003: vague `# Errors` section ──────────────────────────────

/// `load_private` is private and not flagged even though it returns Result.
#[test]
fn doc002_private_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("load_private"),
        "private fns should not be flagged:\n{stderr}"
    );
}

/// The bracket-link variant (`[Error::NotFound]`) passes DOC003.
#[test]
fn doc003_bracket_link_passes() {
    let (stderr, _exit) = run_check_fixture("doc003_vague_errors.rs");
    assert!(
        !stderr.contains("specific_load_bracket"),
        "fn with variant link should not be flagged:\n{stderr}"
    );
}

/// The path-qualified variant (`Error::Timeout` via `::`) passes DOC003.
#[test]
fn doc003_path_qualified_passes() {
    let (stderr, _exit) = run_check_fixture("doc003_vague_errors.rs");
    assert!(
        !stderr.contains("specific_load_path"),
        "fn with path-qualified variant should not be flagged:\n{stderr}"
    );
}

// ── Clean file ────────────────────────────────────────────────────

/// `doc003_vague_errors.rs` warns on `# Errors` sections that name no variant.
#[test]
fn doc003_vague_errors() {
    let (stderr, exit) = run_check_fixture("doc003_vague_errors.rs");

    // DOC003 is a warning - it should not fail the run (only errors fail).
    assert_eq!(exit, 0, "DOC003 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC003", Some("vague_load"));

    let doc003_count = stderr.matches("DOC003").count();
    assert_eq!(
        doc003_count, 1,
        "expected exactly 1 DOC003 finding, got {doc003_count}:\n{stderr}"
    );
}

// ── DOC004: missing `# Arguments` section ─────────────────────────

/// `doc004_missing_arguments.rs` warns on pub fns with parameters but no
/// `# Arguments` section.
#[test]
fn doc004_missing_arguments() {
    let (stderr, exit) = run_check_fixture("doc004_missing_arguments.rs");

    // DOC004 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC004 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC004", Some("greet"));

    let doc004_count = stderr.matches("DOC004").count();
    assert_eq!(
        doc004_count, 1,
        "expected exactly 1 DOC004 finding, got {doc004_count}:\n{stderr}"
    );
}

/// `no_args` has no parameters and is not flagged by DOC004.
#[test]
fn doc004_no_params_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc004_missing_arguments.rs");
    assert!(
        !stderr.contains("no_args"),
        "fn with no parameters should not be flagged:\n{stderr}"
    );
}

// ── DOC005: undocumented parameter ────────────────────────────────

/// `render` documents both parameters and is not flagged by DOC005.
#[test]
fn doc005_documented_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc005_undocumented_param.rs");
    assert!(
        !stderr.contains("render"),
        "fn with complete # Arguments should not be flagged:\n{stderr}"
    );
}

// ── DOC006: doc-comment placeholders ──────────────────────────────

/// `doc005_undocumented_param.rs` warns when a `# Arguments` section omits a
/// parameter name.
#[test]
fn doc005_undocumented_param() {
    let (stderr, exit) = run_check_fixture("doc005_undocumented_param.rs");

    // DOC005 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC005 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC005", Some("build"));
    assert!(
        stderr.contains("fmt"),
        "DOC005 should mention the undocumented param `fmt`:\n{stderr}"
    );

    let doc005_count = stderr.matches("DOC005").count();
    assert_eq!(
        doc005_count, 1,
        "expected exactly 1 DOC005 finding, got {doc005_count}:\n{stderr}"
    );
}

/// `done` has a clean doc comment and is not flagged by DOC006.
#[test]
fn doc006_clean_not_flagged() {
    let (stderr, _exit) = run_check_fixture("doc006_placeholders.rs");
    assert!(
        !stderr.contains("done"),
        "fn with clean doc should not be flagged:\n{stderr}"
    );
}

// ── TEST001: test-function naming ─────────────────────────────────

/// `doc006_placeholders.rs` warns on TODO/FIXME/... in doc comments.
#[test]
fn doc006_placeholders() {
    let (stderr, exit) = run_check_fixture("doc006_placeholders.rs");

    // DOC006 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC006 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC006", Some("todo_task"));
    assert_has_diagnostic(&stderr, "DOC006", Some("fixme_task"));
    assert_has_diagnostic(&stderr, "DOC006", Some("Placeholder"));

    let doc006_count = stderr.matches("DOC006").count();
    assert_eq!(
        doc006_count, 3,
        "expected exactly 3 DOC006 findings, got {doc006_count}:\n{stderr}"
    );
}

/// The `--json` alias is equivalent to `--output-mode json`.
#[test]
fn json_alias_is_equivalent_to_output_mode() {
    let path = fixture_dir().join("clean.rs");
    let alias = run_command(&["--include", "lints", "--json"], &path);
    let mode = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert_eq!(alias.status.code(), mode.status.code());
    assert_eq!(alias.stdout, mode.stdout);
}

/// `--json --dry-run` and `--output-mode json --dry-run` are equivalent and
/// both record the would-be reorder with `severity: "success"`.
#[test]
fn json_dry_run_records_changes_for_both_flags() {
    let path = reorder_fixture_dir().join("fn_interstitial_comment_travels_with_next_before.rs");
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
    assert_eq!(rec["message"], "tables were aligned");
}

/// A non-dry-run reorder in JSON mode reports `success` records in the same
/// document on stdout and writes the file in place.
#[test]
fn json_in_place_run_records_reorder_changes() {
    let expected = fs::read_to_string(
        reorder_fixture_dir().join("fn_interstitial_comment_travels_with_next_after.rs"),
    )
    .unwrap();
    let tmp = temp_file("rs");
    fs::write(
        &tmp,
        fs::read_to_string(
            reorder_fixture_dir().join("fn_interstitial_comment_travels_with_next_before.rs"),
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
    let path = fixture_dir().join("clean.rs");
    let output = run_command(&["--include", "lints", "--output-mode", "json"], &path);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[]\n");
}

/// `--output-mode json --dry-run` succeeds (no clap conflict) on a clean file
/// and emits parseable JSON.
#[test]
fn json_output_combines_with_dry_run() {
    let path = fixture_dir().join("clean.rs");
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
    let path = fixture_dir().join("doc001_missing_docs.rs");
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
        array
            .iter()
            .any(|r| r["severity"] == "error" && r["code"] == "DOC001"),
        "array must contain lint findings, got:\n{stdout}"
    );
    assert!(
        array
            .iter()
            .any(|r| r["severity"] == "success" && r["code"] == "REORDER"),
        "array must contain reorder change records, got:\n{stdout}"
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
    let path = fixture_dir().join("doc001_missing_docs.rs");
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
}

/// `--output-mode json --dry-run` records the would-be reorder as a
/// `severity: "success"` record carrying its move positions on stdout, with no
/// JSON duplicated on stderr.
#[test]
fn json_output_records_reorder_changes() {
    let path = reorder_fixture_dir().join("fn_interstitial_comment_travels_with_next_before.rs");
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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().starts_with('['),
        "stderr must not carry JSON, got:\n{stderr}"
    );
}

// ── JSON output mode ──────────────────────────────────────────────

/// `--output-mode json` prints one JSON array on stdout with every finding
/// for all processed files, using the documented fields and lowercase severity.
#[test]
fn json_output_reports_all_findings() {
    let path = fixture_dir().join("test001_test_naming.rs");
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
                "item_name"
            ]
            .into_iter()
            .collect(),
            "lint record must not carry change-only extras, got: {finding}"
        );
        assert_eq!(finding["severity"], "warning");
        assert_eq!(finding["code"], "TEST001");
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

/// `should_pass_when_valid` is a behavioral name and is not flagged.
#[test]
fn test001_behavioral_not_flagged() {
    let (stderr, _exit) = run_check_fixture("test001_test_naming.rs");
    assert!(
        !stderr.contains("should_pass_when_valid"),
        "behavioral test name should not be flagged:\n{stderr}"
    );
}

/// `test001_test_naming.rs` warns on discouraged test-function names.
#[test]
fn test001_test_naming() {
    let (stderr, exit) = run_check_fixture("test001_test_naming.rs");

    // TEST001 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "TEST001 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "TEST001", Some("test_foo"));
    assert_has_diagnostic(&stderr, "TEST001", Some("test1"));
    assert_has_diagnostic(&stderr, "TEST001", Some("case_1"));

    let test001_count = stderr.matches("TEST001").count();
    assert_eq!(
        test001_count, 3,
        "expected exactly 3 TEST001 findings, got {test001_count}:\n{stderr}"
    );
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

// ── DOC001: missing doc comments ──────────────────────────────────

/// The directory holding fix fixtures.
fn fix_fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("fix")
}

/// The directory holding reorder fixtures.
fn reorder_fixture_dir() -> std::path::PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("reorder")
}

/// Run `rust-llm-tidy check <fixture>` and return (stderr, exit_code).
fn run_check_fixture(name: &str) -> (String, i32) {
    let path = fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-lint-dir-{}-{}", pid, seq))
}

/// Create a numbered temporary file path with the given extension.
fn temp_file(ext: &str) -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-all-{}-{}.{}", pid, seq, ext))
}

/// The directory holding lint fixtures.
fn fixture_dir() -> std::path::PathBuf {
    manifest_dir().join("tests").join("fixtures").join("doc")
}

/// Build `rust-llm-tidy <args> <path>` and run it, returning captured output.
fn run_command(args: &[&str], path: &std::path::Path) -> std::process::Output {
    let mut cmd = Command::new(binary());
    cmd.args(["--no-config"]).args(args).arg(path);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {}: {e}", path.display()))
}

/// Return the path to the `rust-llm-tidy` debug binary.
///
/// Prefers `CARGO_BIN_EXE_rust_llm_tidy` (set by `cargo test` at runtime);
/// falls back to the sibling of the test binary under `target/<triple>/debug/`,
/// since the test binary lives in `target/<triple>/debug/deps/`.
fn binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy") {
        return std::path::PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("current_exe must resolve");
    // Drop `<test-name>-<hash>` and `deps/` to reach `target/<triple>/debug/`.
    path.pop();
    path.pop();
    path.join("rust-llm-tidy")
}

/// Return `CARGO_MANIFEST_DIR` for resolving fixture paths.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
