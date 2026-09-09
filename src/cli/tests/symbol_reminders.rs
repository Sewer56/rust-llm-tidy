//! Exercise documented symbol configurations through the CLI with local fixtures.

use common::binary;
use rstest::rstest;
use std::fs;
use std::process::Command;

mod common;

#[rstest]
#[case::default("", false, false)]
#[case::entry_changed(", scope: changed_lines", false, false)]
#[case::entry_all(", scope: all", false, true)]
#[case::override_config("", true, true)]
#[case::override_entry(", scope: changed_lines", true, true)]
fn cli_should_prioritize_all_lines_over_entry_and_config_scopes(
    #[case] entry: &str,
    #[values("reminder", "hint", "warning", "error")] severity: &str,
    #[case] all_lines: bool,
    #[case] reported: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("input.rs"), "fn f() { needle(); }").unwrap();
    fs::write(
        directory.path().join("config.yml"),
        format!(
            "perf_hints: []\nlint_scopes: {{SYM: changed_lines}}\nsymbol_rules: [{{regex: needle, title: Review, message: finding, severity: {severity}{entry}}}]"
        ),
    ).unwrap();
    let mut command = Command::new(binary());
    command
        .current_dir(directory.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args([
            "--config",
            "config.yml",
            "--include",
            "SYM",
            "--json",
            "input.rs",
        ]);
    if all_lines {
        command.arg("--all-lines");
    }

    let output = command.output().unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(records.len(), usize::from(reported), "{output:?}");
    assert_eq!(output.status.success(), !(reported && severity == "error"));
    if reported {
        assert_eq!(records[0]["severity"], severity);
    }
}

#[rstest]
#[case::capacity_include("--include", "PERF001")]
#[case::array_include("--include", "PERF002")]
#[case::capacity_exclude("--exclude", "PERF001")]
#[case::array_exclude("--exclude", "PERF002")]
fn cli_should_reject_performance_families_as_lint_controls(#[case] flag: &str, #[case] code: &str) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("config.yml"), "{}").unwrap();
    fs::write(directory.path().join("input.rs"), "fn f() {}").unwrap();

    let output = Command::new(binary())
        .current_dir(directory.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args(["--config", "config.yml", flag, code, "input.rs"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(code));
}

#[rstest]
#[case::original_lint_scope("--lint-scope")]
#[case::intermediate_reminder_scope("--reminder-scope")]
fn cli_should_reject_retired_scope_flags(#[case] flag: &str) {
    let directory = tempfile::tempdir().unwrap();

    let output = Command::new(binary())
        .current_dir(directory.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args([flag, "all"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(&format!("unexpected argument '{flag}'")),
        "{error}"
    );
}

#[rstest]
#[case::omitted("", 1, 1)]
#[case::both("perf_hints: [PERF001, PERF002]\n", 1, 1)]
#[case::capacity("perf_hints: [PERF001]\n", 1, 0)]
#[case::array("perf_hints: [PERF002]\n", 0, 1)]
#[case::disabled("perf_hints: []\n", 0, 0)]
#[case::duplicates("perf_hints: [PERF001, PERF001, PERF002]\n", 1, 1)]
fn reminders_should_select_builtins_without_disabling_custom_rules(
    #[case] selection: &str,
    #[case] capacity: usize,
    #[case] arrays: usize,
) {
    let directory = tempfile::tempdir().unwrap();
    let source = "class C { void M() { new List<int>(); var a = new int[3]; Custom(); } }";
    let config = format!(
        "{selection}symbol_rules: [{{symbol: Custom, title: Custom API, message: custom guidance}}]"
    );
    fs::write(directory.path().join("config.yml"), config).unwrap();
    fs::write(directory.path().join("input.cs"), source).unwrap();

    let output = Command::new(binary())
        .current_dir(directory.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args([
            "--config",
            "config.yml",
            "--include",
            "SYM",
            "--all-lines",
            "--json",
            "input.cs",
        ])
        .output()
        .unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(records.len(), capacity + arrays + 1);
    assert!(records.iter().all(|record| record["code"] == "SYM"));
    assert_eq!(
        records
            .iter()
            .filter(|record| record["title"] == "PERF001: API performance reminder")
            .count(),
        capacity
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["title"] == "PERF002: array initialization reminder")
            .count(),
        arrays
    );
    assert!(records.iter().any(|record| record["title"] == "Custom API" && record["message"] == "custom guidance"));
}

#[rstest]
#[case::rust("input.RS", "fn f() { Vec::new(); }", "Vec::new", "RS", None)]
#[case::array(
    "input.CS",
    "class C { void M() { var a = new int[3]; } }",
    "new[]",
    "CS",
    Some("explicit_sized_vector")
)]
fn symbol_reminders_should_render_documented_messages_in_all_lines_audit(
    #[case] filename: &str,
    #[case] source: &str,
    #[case] symbol: &str,
    #[case] extension: &str,
    #[case] array_kind: Option<&str>,
) {
    let directory = tempfile::tempdir().unwrap();
    let message = "Review initialization.\nWhy: Initial values may matter.\nSuggestions:\n- Keep required initialization.\n";
    let mut config = format!(
        "perf_hints: []\nsymbol_rules:\n  - symbol: {symbol}\n    title: Initialization\n    extensions: [{extension}]\n    message: |\n      Review initialization.\n      Why: Initial values may matter.\n      Suggestions:\n      - Keep required initialization.\n"
    );
    if let Some(kind) = array_kind {
        config.push_str(&format!(
            "    array_kind: {kind}\n    no_initializer: true\n"
        ));
    }
    fs::write(directory.path().join("config.yml"), config).unwrap();
    fs::write(directory.path().join(filename), source).unwrap();

    let output = Command::new(binary())
        .current_dir(directory.path())
        .env_remove("RUST_LLM_TIDY_DIFF_BASE")
        .args([
            "--config",
            "config.yml",
            "--include",
            "SYM",
            "--all-lines",
            "--json",
            filename,
        ])
        .output()
        .unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["code"], "SYM");
    assert_eq!(records[0]["severity"], "reminder");
    assert_eq!(records[0]["message"], message);
    assert_eq!(records[0]["title"], "Initialization");
    assert_eq!(records[0]["line"], 1);
    assert_eq!(
        fs::read_to_string(directory.path().join(filename)).unwrap(),
        source
    );
}
