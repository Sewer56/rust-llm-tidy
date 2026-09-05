//! Rust lint tests for the `check` subcommand over the `.rs` fixtures in
//! `tests/fixtures/doc/rust/`.
//!
//! Every test runs the built CLI binary with `--include lints` and asserts
//! on its exit code and stderr diagnostics (the JSON-composition test
//! asserts on stdout); the shared runner helpers live in `mod.rs`.

use super::{assert_has_diagnostic, run_command, run_rust_fixture, rust_fixture_dir, temp_file};
use std::fs;

/// `clean.rs` is fully documented and produces zero diagnostics.
#[test]
fn clean_file_no_diagnostics() {
    let (stderr, exit) = run_rust_fixture("clean.rs");
    assert_eq!(exit, 0, "clean file should pass");
    assert!(
        stderr.is_empty(),
        "clean file should produce no diagnostics, got:\n{stderr}"
    );
}

// ── DOC001: missing doc comments ──────────────────────────────────

/// The documented function in `doc001_missing_docs.rs` is not flagged.
#[test]
fn doc001_documented_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc001_missing_docs.rs");
    assert!(
        !stderr.contains("`documented`"),
        "documented functions should not be flagged:\n{stderr}"
    );
}

/// `doc001_missing_docs.rs` flags every undocumented non-private documentable
/// item, skips private items and `pub use`.
#[test]
fn doc001_missing_docs() {
    let (stderr, exit) = run_rust_fixture("doc001_missing_docs.rs");
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
    let (stderr, _exit) = run_rust_fixture("doc001_missing_docs.rs");
    assert!(
        !stderr.contains("`helper`"),
        "private functions should not be flagged:\n{stderr}"
    );
}

/// `pub use` is not a documentable kind and must not be flagged.
#[test]
fn doc001_pub_use_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc001_missing_docs.rs");
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
    let (stderr, _exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("save"),
        "fn with complete # Errors should not be flagged:\n{stderr}"
    );
}

/// `doc002_missing_errors_section.rs` flags pub fns returning Result without
/// a `# Errors` section.
#[test]
fn doc002_missing_errors_section() {
    let (stderr, exit) = run_rust_fixture("doc002_missing_errors_section.rs");
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
    let (stderr, _exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("count"),
        "non-Result pub fns should not be flagged:\n{stderr}"
    );
}

/// `load_private` is private and not flagged even though it returns Result.
#[test]
fn doc002_private_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc002_missing_errors_section.rs");
    assert!(
        !stderr.contains("load_private"),
        "private fns should not be flagged:\n{stderr}"
    );
}

// ── DOC003: vague `# Errors` section ──────────────────────────────

/// The bracket-link variant (`[Error::NotFound]`) passes DOC003.
#[test]
fn doc003_bracket_link_passes() {
    let (stderr, _exit) = run_rust_fixture("doc003_vague_errors.rs");
    assert!(
        !stderr.contains("specific_load_bracket"),
        "fn with variant link should not be flagged:\n{stderr}"
    );
}

/// The path-qualified variant (`Error::Timeout` via `::`) passes DOC003.
#[test]
fn doc003_path_qualified_passes() {
    let (stderr, _exit) = run_rust_fixture("doc003_vague_errors.rs");
    assert!(
        !stderr.contains("specific_load_path"),
        "fn with path-qualified variant should not be flagged:\n{stderr}"
    );
}

/// `doc003_vague_errors.rs` warns on `# Errors` sections that name no variant.
#[test]
fn doc003_vague_errors() {
    let (stderr, exit) = run_rust_fixture("doc003_vague_errors.rs");

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
    let (stderr, exit) = run_rust_fixture("doc004_missing_arguments.rs");

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
    let (stderr, _exit) = run_rust_fixture("doc004_missing_arguments.rs");
    assert!(
        !stderr.contains("no_args"),
        "fn with no parameters should not be flagged:\n{stderr}"
    );
}

// ── DOC005: undocumented parameter ────────────────────────────────

/// `render` documents both parameters and is not flagged by DOC005.
#[test]
fn doc005_documented_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc005_undocumented_param.rs");
    assert!(
        !stderr.contains("render"),
        "fn with complete # Arguments should not be flagged:\n{stderr}"
    );
}

/// `doc005_undocumented_param.rs` warns when a `# Arguments` section omits a
/// parameter name.
#[test]
fn doc005_undocumented_param() {
    let (stderr, exit) = run_rust_fixture("doc005_undocumented_param.rs");

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

// ── DOC006: doc-comment placeholders ──────────────────────────────

/// `done` has a clean doc comment and is not flagged by DOC006.
#[test]
fn doc006_clean_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("doc006_placeholders.rs");
    assert!(
        !stderr.contains("done"),
        "fn with clean doc should not be flagged:\n{stderr}"
    );
}

/// `doc006_placeholders.rs` warns on TODO/FIXME/TBD doc-comment markers.
#[test]
fn doc006_placeholders() {
    let (stderr, exit) = run_rust_fixture("doc006_placeholders.rs");

    // DOC006 is a warning - it should not fail the run.
    assert_eq!(exit, 0, "DOC006 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC006", Some("todo_task"));
    assert_has_diagnostic(&stderr, "DOC006", Some("fixme_task"));
    assert_has_diagnostic(&stderr, "DOC006", Some("tbd_task"));
    // A literal `...` is idiomatic prose, not a placeholder marker.
    assert!(
        !stderr.contains("Placeholder"),
        "ellipsis-only docs should not be flagged:\n{stderr}"
    );

    let doc006_count = stderr.matches("DOC006").count();
    assert_eq!(
        doc006_count, 3,
        "expected exactly 3 DOC006 findings, got {doc006_count}:\n{stderr}"
    );
}

// ── Text budgets ─────────────────────────────────────────────────

/// Rust block and attribute doc prose fires the text budgets with
/// original file lines: TEXT001 errors on the over-budget `/** */` and
/// `#[doc = "..."]` paragraphs, and TEXT002 warns on the 81-char block
/// and attribute lines.
#[test]
fn rs_block_and_attribute_docs_fire_text_budgets() {
    let (stderr, exit) = run_rust_fixture("text-001_text-002_block_attr_budgets.rs");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the block doc's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":25: error[TEXT001]"),
        "TEXT001 must report at the attribute paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":8: warning[TEXT002]"),
        "TEXT002 must report at the over-long attribute line:\n{stderr}"
    );
    assert!(
        stderr.contains(":12: warning[TEXT002]"),
        "TEXT002 must report at the over-long block doc line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "exactly the block and attribute paragraphs, never the plain block:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        2,
        "exactly the block and attribute lines, never the plain block:\n{stderr}"
    );
    assert!(
        !stderr.contains("DOC001"),
        "the fixture's private fns stay out of DOC001:\n{stderr}"
    );
}

/// The CLI's rendered rs findings equal the direct composition of the
/// tree-sitter checks (`run_all`) and the rs text checks
/// (`rust_text_regions::text_checks`) over the same file: rs dispatch
/// adds nothing and drops nothing.
///
/// The rs text checks cover line comments plus `/** */` and
/// `#[doc = "..."]` docs.
#[test]
fn rs_diagnostics_match_direct_check_composition() {
    for name in [
        "doc001_missing_docs.rs",
        "text-001_text-002_block_attr_budgets.rs",
    ] {
        let path = rust_fixture_dir().join(name);
        let source = fs::read_to_string(&path).unwrap();

        // Path A: the CLI pipeline's rendered JSON findings.
        let output = run_command(&["--include", "lints", "--output-mode", "json"], &path);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let rendered: Vec<(usize, String, String)> =
            serde_json::from_str::<serde_json::Value>(&stdout)
                .expect("JSON diagnostics must parse")
                .as_array()
                .expect("diagnostics must be an array")
                .iter()
                .map(|f| {
                    (
                        f["line"].as_u64().expect("line must be a number") as usize,
                        f["severity"].as_str().expect("severity").to_string(),
                        f["code"].as_str().expect("code").to_string(),
                    )
                })
                .collect();

        // Path B: the two check sources composed directly over the same
        // source.
        let parsed = rust_llm_tidy_model::parse::parse_source(&source).unwrap();
        let mut expected = rust_llm_tidy_lint::check::run_all(&parsed);
        expected.extend(rust_llm_tidy_lang::rust_text_regions::text_checks(&parsed));
        let expected: Vec<(usize, String, String)> = expected
            .iter()
            .map(|d| {
                let sev = match d.severity {
                    rust_llm_tidy_lint::Severity::Error => "error",
                    rust_llm_tidy_lint::Severity::Warning => "warning",
                };
                (d.line, sev.to_string(), d.code.to_string())
            })
            .collect();

        assert_eq!(
            rendered, expected,
            "{name}: CLI rs dispatch must render exactly run_all + text_checks"
        );
    }
}

/// Rust comments flow through the same text checks: an 81-char `///` line
/// warns with TEXT002 while tree-sitter checks stay quiet on a private fn.
#[test]
fn rs_long_doc_comment_warns_text002() {
    let path = temp_file("rs");
    fs::write(&path, format!("/// {}\nfn hidden() {{}}\n", "w".repeat(81))).unwrap();

    let output = run_command(&["--include", "lints"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT002 warnings must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":1: warning[TEXT002]"),
        "expected a TEXT002 warning for the over-limit comment, got:\n{stderr}"
    );
}

// ── TEST001: test-function naming ─────────────────────────────────

/// `should_pass_when_valid` is a behavioral name and is not flagged.
#[test]
fn test001_behavioral_not_flagged() {
    let (stderr, _exit) = run_rust_fixture("test001_test_naming.rs");
    assert!(
        !stderr.contains("should_pass_when_valid"),
        "behavioral test name should not be flagged:\n{stderr}"
    );
}

/// `test001_test_naming.rs` warns on discouraged test-function names.
#[test]
fn test001_test_naming() {
    let (stderr, exit) = run_rust_fixture("test001_test_naming.rs");

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
