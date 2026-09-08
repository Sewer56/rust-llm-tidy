//! Exercise documented symbol configurations through the CLI with local fixtures.

use common::binary;
use rstest::rstest;
use std::fs;
use std::process::Command;

mod common;

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
        "symbol_rules:\n  - symbol: {symbol}\n    extensions: [{extension}]\n    message: |\n      Review initialization.\n      Why: Initial values may matter.\n      Suggestions:\n      - Keep required initialization.\n"
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
            "SYM001",
            "--lint-scope",
            "all",
            "--json",
            filename,
        ])
        .output()
        .unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["code"], "SYM001");
    assert_eq!(records[0]["severity"], "reminder");
    assert_eq!(records[0]["message"], message);
    assert_eq!(records[0]["line"], 1);
    assert_eq!(
        fs::read_to_string(directory.path().join(filename)).unwrap(),
        source
    );
}
