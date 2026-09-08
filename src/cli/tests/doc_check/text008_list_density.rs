//! TEXT008 list budgets and rule selection through the CLI with isolated config.

use super::{common::binary, temp_dir};
use rstest::rstest;
use std::fs;
use std::process::{Command, Output};

/// Public source-line budget; the core rule's constant stays private.
const ACCEPTANCE_LIST_LINE_BUDGET: usize = 10;

// List budget boundaries.

/// Over-budget lists warn once at their first line.
#[rstest]
#[case::six_wrapped("- item\n  tail\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET / 2 + 1), 1)]
#[case::seven_single("- item\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET - 3), 0)]
#[case::eight_single("- item\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET - 2), 0)]
#[case::five_wrapped_at_budget("- item\n  tail\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET / 2), 0)]
#[case::ten_single_at_budget("- item\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET), 0)]
#[case::eleven_single_over_budget("- item\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET + 1), 1)]
fn cli_should_follow_list_source_line_budget(#[case] list: String, #[case] warnings: usize) {
    let source = format!("Intro.\n\n{list}");

    let output = run(&source, &["--include", "TEXT008"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("TEXT008").count(), warnings, "{stderr}");
    assert_eq!(
        stderr.matches(":3: warning[TEXT008]").count(),
        warnings,
        "{stderr}"
    );
}

// CLI rule selection.

/// Inclusion enables TEXT008; exclusion suppresses its warning.
#[rstest]
#[case::default_run(&[], 1)]
#[case::included_code(&["--include", "TEXT008"], 1)]
#[case::excluded_code(&["--exclude", "TEXT008"], 0)]
#[case::excluded_included_code(&["--include", "TEXT008", "--exclude", "TEXT008"], 0)]
fn cli_should_follow_rule_selection(#[case] args: &[&str], #[case] warnings: usize) {
    let source = "- item\n  tail\n".repeat(ACCEPTANCE_LIST_LINE_BUDGET / 2 + 1);

    let output = run(&source, args);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(
        stderr.matches("warning[TEXT008]").count(),
        warnings,
        "{stderr}"
    );
}

/// Run temporary Markdown with empty config, then remove the fixtures.
fn run(source: &str, args: &[&str]) -> Output {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let path = dir.join("source.md");
    fs::write(&path, source).unwrap();
    let config = dir.join(".rust-llm-tidy.yml");
    fs::write(&config, "{}").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(config)
        .arg("--dry-run")
        .args(args)
        .arg(path)
        .output();

    fs::remove_dir_all(dir).unwrap();
    output.expect("failed to spawn rust-llm-tidy")
}
