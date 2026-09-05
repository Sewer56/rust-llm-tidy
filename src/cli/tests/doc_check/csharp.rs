//! C# lint tests: the XML doc dialect over the same lint codes as Rust.
//!
//! The lint tests run the built CLI binary with `--include lints` on a
//! fixture in `tests/fixtures/doc/csharp/`; the JSON record tests also
//! cover `--include reorder` over `tests/fixtures/reorder/csharp/`.
//!
//! The shared runner helpers live in `mod.rs`.

use super::{assert_has_diagnostic, manifest_dir, reorder_fixture_dir, run_command};

// ── DOC001: missing doc comments ──────────────────────────────────

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

// ── DOC002: missing `<exception>` tag ────────────────────────────

/// DOC002 recursion: a caller with no `throw` of its own is flagged
/// for calling a same-file thrower, transitively; the private thrower,
/// framework calls, and tagged callers stay silent. Findings keep
/// document order and error severity.
#[test]
fn csharp_doc002_errors_on_indirect_throwers() {
    let (stderr, exit) = run_csharp_fixture("doc002_indirect_exception.cs");

    assert_ne!(exit, 0, "DOC002 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains("error[DOC002]"),
        "DOC002 carries error severity:\n{stderr}"
    );
    assert_has_diagnostic(&stderr, "DOC002", Some("Load"));
    assert_has_diagnostic(&stderr, "DOC002", Some("LoadTwice"));
    assert!(
        !stderr.contains("`Validate`")
            && !stderr.contains("`Parse`")
            && !stderr.contains("`LoadGuarded`"),
        "private throwers, framework calls, and tagged callers pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC002").count(),
        2,
        "expected exactly 2 DOC002 findings:\n{stderr}"
    );
    let direct = stderr.find("(fn `Load`)").expect("Load must be named");
    let transitive = stderr
        .find("(fn `LoadTwice`)")
        .expect("LoadTwice must be named");
    assert!(
        direct < transitive,
        "findings stay in document order:\n{stderr}"
    );
}

/// DOC002 errors on the documented non-private thrower without an
/// `<exception>` tag; the tagged and private throwers pass.
#[test]
fn csharp_doc002_errors_on_untagged_throwers() {
    let (stderr, exit) = run_csharp_fixture("doc002_missing_exception.cs");

    assert_ne!(exit, 0, "DOC002 errors must fail the run:\n{stderr}");
    assert_has_diagnostic(&stderr, "DOC002", Some("Untagged"));
    assert!(
        stderr.contains("error[DOC002]"),
        "DOC002 carries error severity:\n{stderr}"
    );
    assert!(
        !stderr.contains("`Tagged`") && !stderr.contains("`Hidden`"),
        "tagged and private throwers pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC002").count(),
        1,
        "expected exactly 1 DOC002 finding:\n{stderr}"
    );
}

// ── DOC003: vague `<exception>` cref ─────────────────────────────

/// DOC003 warns when `<exception>` tags carry no concrete `cref`.
#[test]
fn csharp_doc003_warns_on_vague_exception_crefs() {
    let (stderr, exit) = run_csharp_fixture("doc003_vague_exception.cs");

    assert_eq!(exit, 0, "DOC003 warnings must not fail the run");
    assert_has_diagnostic(&stderr, "DOC003", Some("Vague"));
    assert!(
        !stderr.contains("`Concrete`"),
        "a concrete cref passes:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC003").count(),
        1,
        "expected exactly 1 DOC003 finding:\n{stderr}"
    );
}

// ── DOC004: missing `<param>` tag ────────────────────────────────

/// DOC004 warns on the parameterized member without `<param>` tags.
#[test]
fn csharp_doc004_warns_on_missing_param_tags() {
    let (stderr, exit) = run_csharp_fixture("doc004_missing_param.cs");

    assert_eq!(exit, 0, "DOC004 warnings must not fail the run");
    assert_has_diagnostic(&stderr, "DOC004", Some("Greet"));
    assert!(
        !stderr.contains("`Greeted`") && !stderr.contains("`NoArgs`"),
        "tagged and parameterless members pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC004").count(),
        1,
        "expected exactly 1 DOC004 finding:\n{stderr}"
    );
}

// ── DOC005: undocumented parameter ────────────────────────────────

/// DOC005 names the parameter the `<param>` tags omitted.
#[test]
fn csharp_doc005_names_the_undocumented_param() {
    let (stderr, exit) = run_csharp_fixture("doc005_undocumented_param.cs");

    assert_eq!(exit, 0, "DOC005 warnings must not fail the run");
    assert_has_diagnostic(&stderr, "DOC005", Some("Build"));
    assert!(
        stderr.contains("`format`"),
        "DOC005 must name the omitted parameter:\n{stderr}"
    );
    assert!(
        !stderr.contains("`Built`"),
        "fully documented members pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC005").count(),
        1,
        "expected exactly 1 DOC005 finding:\n{stderr}"
    );
}

// ── DOC006: doc-comment placeholders ──────────────────────────────

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

// ── JSON records ─────────────────────────────────────────────────

/// `--include reorder --output-mode json --dry-run` on a `.cs` file
/// records the would-be using hoist and member reorder with
/// `severity: "success"` and no title, exactly like the Rust reorder
/// records.
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
        output.status.success(),
        "JSON dry-run should succeed: {}",
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

    let keys: std::collections::BTreeSet<&str> = array[0]
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

// ── TEST001: test-function naming ─────────────────────────────────

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

// ── Text budgets ─────────────────────────────────────────────────

/// C# text budgets fire with original file lines: TEXT001 errors on an
/// over-budget summary paragraph at its first prose line, and TEXT002
/// warns on a line whose tag-stripped inner text exceeds 80 chars.
#[test]
fn csharp_text_budgets_fire_with_original_lines() {
    let (stderr, exit) = run_csharp_fixture("text-001_text-002_text_budgets.cs");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":10: error[TEXT001]"),
        "TEXT001 must report at the summary's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":19: warning[TEXT002]"),
        "TEXT002 must report at the over-long measured line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "expected exactly 1 TEXT001 finding:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "expected exactly 1 TEXT002 finding:\n{stderr}"
    );
    assert!(
        !stderr.contains("DOC001") && !stderr.contains("DOC004"),
        "the fixture is otherwise documented:\n{stderr}"
    );
}

/// C# text checks stay quiet on the probe classes: idiomatic XML docs,
/// long `cref`/`name` attribute values, `<code>`/`<example>` blocks, and
/// verbatim string content produce no TEXT001/TEXT002 findings.
#[test]
fn csharp_text_probes_stay_quiet() {
    let (stderr, exit) = run_csharp_fixture("doc_text_quiet_probes.cs");

    assert_eq!(
        exit, 0,
        "the probe fixture must be clean across every C# lint"
    );
    assert!(
        stderr.is_empty(),
        "idiomatic docs and string content must stay unmeasured:\n{stderr}"
    );
}

// ── Helpers ───────────────────────────────────────────────────────

/// Run `rust-llm-tidy --include lints` on a C# fixture and return its
/// (stderr, exit_code).
fn run_csharp_fixture(name: &str) -> (String, i32) {
    let path = csharp_fixture_dir().join(name);
    let output = run_command(&["--include", "lints"], &path);
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// The directory holding the C# lint fixtures.
fn csharp_fixture_dir() -> std::path::PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("doc")
        .join("csharp")
}
