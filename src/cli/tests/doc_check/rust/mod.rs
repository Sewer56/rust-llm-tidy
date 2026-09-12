//! Rust lint tests for the `check` subcommand over the `.rs` fixtures in
//! `tests/fixtures/doc/rust/`.
//!
//! Rule tests select their relevant lint codes and assert on exit codes
//! and stderr diagnostics. Clean-file and composition tests run all lints;
//! the composition test also parses its JSON stdout.
//!
//! Child modules:
//! - `doc001_missing_docs`: DOC001 undocumented public items
//! - `doc002_missing_errors_section`: DOC002 Result fns without `# Errors`
//! - `doc003_vague_errors`: DOC003 `# Errors` sections naming no variant
//! - `doc004_missing_arguments`: DOC004 params without `# Arguments`
//! - `doc005_undocumented_param`: DOC005 omitted parameter names
//! - `doc006_placeholder`: DOC006 doc-comment placeholder markers
//! - `doc008_error_variant_order`: DOC008 out-of-order error variants
//! - `doc009_missing_module_docs`: missing module documentation
//! - `doc010_section_order`: DOC010 out-of-canonical-order doc sections
//!
//! Size, naming, text, and perf modules:
//!
//! - `mod001_module_size`: MOD001 rust-specific counting and selection
//! - `mod003_qualified_path`: MOD003 fully-qualified path hints
//! - `len001_method_length`: LEN001 end-to-end acceptance and selection
//! - `perf001_allocation_hints`: built-in PERF001 capacity reminders via SYM
//! - `test001_test_naming`: TEST001 discouraged test-function names
//! - `test002_test_summary`: TEST002 missing test summary comments
//! - `text001_paragraph_size`: TEXT001 over-budget doc paragraphs
//! - `text002_line_length`: TEXT002 over-long doc lines
//! - `text003_sentence_length`: TEXT003 over-budget doc sentences
//! - `text004_header_opener`: TEXT004 three-sentence doc openers

use crate::{run_command, rust_fixture_dir};
use rust_llm_tidy::languages::LanguageBackend;
use std::fs;

mod doc001_missing_docs;
mod doc002_missing_errors_section;
mod doc003_vague_errors;
mod doc004_missing_arguments;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod doc008_error_variant_order;
mod doc009_missing_module_docs;
mod doc010_section_order;
mod len001_method_length;
mod mod001_module_size;
mod mod003_qualified_path;
mod perf001_allocation_hints;
mod test001_test_naming;
mod test002_test_summary;
mod text001_paragraph_size;
mod text002_line_length;
mod text003_sentence_length;
mod text004_header_opener;

/// `clean.rs` is fully documented and produces zero diagnostics.
#[test]
fn clean_file_no_diagnostics() {
    let (stderr, exit) = run_rust_fixture("clean.rs", "lints");
    assert_eq!(exit, 0, "clean file should pass");
    assert!(
        stderr.is_empty(),
        "clean file should produce no diagnostics, got:\n{stderr}"
    );
}

/// The CLI's rendered rs findings equal the Rust backend's lint
/// composition over the same file.
///
/// That composition is the item rules (DOC*, TEST001) plus the rs text
/// checks.
///
/// The rs text checks cover line comments plus `/** */` and
/// `#[doc = "..."]` docs.
///
/// rs dispatch adds nothing and drops nothing. All-line reporting includes
/// TEXT007 AI reminders, matching the unfiltered backend composition.
#[test]
fn rs_diagnostics_match_direct_check_composition() {
    for name in [
        "doc001_missing_docs.rs",
        "text-001_text-002_block_attr_budgets.rs",
        "doc001_doc002_doc004_text002_mixed.rs",
    ] {
        let path = rust_fixture_dir().join(name);
        let source = fs::read_to_string(&path).unwrap();

        // Path A: the CLI pipeline's rendered JSON findings.
        let output = run_command(
            &[
                "--include",
                "lints",
                "--include",
                "TEXT007",
                "--output-mode",
                "json",
            ],
            &path,
        );
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

        // Path B: the Rust backend's lint composition called directly over
        // the same source.
        let parsed = rust_llm_tidy::languages::RustBackend
            .parse(&source)
            .unwrap();
        let expected = rust_llm_tidy::languages::rust::RustBackend.lint(&parsed);
        let expected: Vec<(usize, String, String)> = expected
            .iter()
            .map(|d| {
                let sev = match d.severity {
                    rust_llm_tidy::reporting::Severity::Error => "error",
                    rust_llm_tidy::reporting::Severity::Warning => "warning",
                    rust_llm_tidy::reporting::Severity::Hint => "hint",
                    rust_llm_tidy::reporting::Severity::Reminder => "reminder",
                    rust_llm_tidy::reporting::Severity::AiReminder => "ai_reminder",
                };
                (d.line, sev.to_string(), d.code.to_string())
            })
            .collect();

        assert_eq!(
            rendered, expected,
            "{name}: CLI rs dispatch must render exactly the backend lint composition"
        );
    }
}

/// Run the selected comma-separated lint codes on a Rust fixture and return
/// its (stderr, exit_code).
fn run_rust_fixture(name: &str, codes: &str) -> (String, i32) {
    let path = rust_fixture_dir().join(name);
    let args: Vec<_> = codes
        .split(',')
        .flat_map(|code| ["--include", code])
        .collect();
    let output = run_command(&args, &path);

    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}
