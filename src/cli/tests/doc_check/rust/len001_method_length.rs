//! LEN001 end-to-end acceptance through the real CLI pipeline.
//!
//! Covers the default firing point, a configured threshold override,
//! the rendered split advice, and `--include`/`--exclude` gating, with
//! a config-file runner mirroring the MOD001 suite.

use crate::common::binary;
use rstest::rstest;
use rust_llm_tidy::config::MethodLengthConfig;
use std::fs;
use std::process::{Command, Output};

/// The rule follows the same CLI selection as every other code.
#[rstest]
#[case::default_run(&["--dry-run"], true)]
#[case::lints_group(&["--include", "lints"], true)]
#[case::code_only(&["--include", "LEN001"], true)]
#[case::excluded_code(&["--exclude", "LEN001"], false)]
#[case::excluded_group(&["--exclude", "lints"], false)]
#[case::other_code_only(&["--include", "TEXT001"], false)]
fn len001_should_follow_cli_rule_selection(#[case] args: &[&str], #[case] fires: bool) {
    let source = fn_with_body_lines(MethodLengthConfig::default().max_lines + 1);

    let output = run(&source, "source.rs", "{}", args);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("hint[LEN001]"), fires, "{stderr}");
}

/// Config selection gates LEN001 like any other rule.
#[rstest]
#[case::included("include:\n  - rules: [LEN001]\n", true)]
#[case::excluded("exclude:\n  - rules: [LEN001]\n", false)]
#[case::excluded_group("exclude:\n  - rules: [lints]\n", false)]
fn len001_should_follow_config_rule_selection(#[case] selection: &str, #[case] fires: bool) {
    let source = fn_with_body_lines(MethodLengthConfig::default().max_lines + 1);

    let output = run(&source, "source.rs", selection, &["--dry-run"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("hint[LEN001]"), fires, "{stderr}");
}

/// Only bodies strictly over the resolved budget produce hints.
/// Hints exit successfully.
#[rstest]
#[case::default_at_budget(MethodLengthConfig::default().max_lines, None, false)]
#[case::default_over_budget(MethodLengthConfig::default().max_lines + 1, None, true)]
#[case::custom_at_budget(3, Some(3), false)]
#[case::custom_over_budget(4, Some(3), true)]
#[case::default_under_budget(4, None, false)]
#[case::loosened_budget(MethodLengthConfig::default().max_lines + 1, Some(300), false)]
fn len001_should_follow_the_resolved_budget(
    #[case] body_lines: usize,
    #[case] max_lines: Option<usize>,
    #[case] fires: bool,
) {
    let source = fn_with_body_lines(body_lines);
    let config = max_lines.map_or_else(
        || "{}".to_owned(),
        |max_lines| format!("method_length:\n  max_lines: {max_lines}\n"),
    );
    let budget = max_lines.unwrap_or(MethodLengthConfig::default().max_lines);

    let output = run(&source, "source.rs", &config, &["--include", "LEN001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(
        stderr.matches("LEN001").count(),
        usize::from(fires),
        "{stderr}"
    );
    if fires {
        assert!(stderr.contains(":2: hint[LEN001]"), "{stderr}");
        assert!(
            stderr.contains(&format!("fn `oversized` has {body_lines} body lines")),
            "{stderr}"
        );
        assert!(
            stderr.contains(&format!(
                "over the {budget}-line budget (method_length.max_lines)"
            )),
            "{stderr}"
        );
    }
}

/// Text and JSON retain the same full hint message and exit successfully.
#[test]
fn len001_should_render_the_full_hint_in_text_and_json() {
    let source = fn_with_body_lines(4);
    let config = "method_length:\n  max_lines: 3\n";
    let expected = indoc::indoc! {"
        fn `oversized` has 4 body lines (blank and comment-only lines excluded),
        over the 3-line budget (method_length.max_lines).
        Why:
        - Long functions can make readers track too much control flow and local state.
        - Named, cohesive steps can help readers follow the flow without tracking every detail.
        Suggestions:
        - Consider extracting cohesive steps into functions named for what they do,
          so the outer function reads as an overview of the flow.
        - Keep closely related work together. Avoid new types, forwarding wrappers,
          or a wider public API solely to shorten the body.
        - Preserve behavior and performance. Avoid extra allocations, cloning, or
          repeated work; measure performance-sensitive changes.
        - Mark extracted functions as `#[inline]` if needed.
        - Inner functions still count toward the enclosing body.
        - Keep the body intact if splitting would make it harder to follow or slower."};

    let output = run(&source, "source.rs", config, &["--include", "LEN001"]);
    let json_output = run(
        &source,
        "source.rs",
        config,
        &["--include", "LEN001", "--json"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(json_output.status.success(), "{:?}", json_output);
    let records: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let records = records.as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["code"], "LEN001");
    assert_eq!(records[0]["severity"], "hint");
    assert_eq!(records[0]["message"], expected);
    let message = records[0]["message"].as_str().unwrap();
    assert!(
        stderr.contains(&format!(":2: hint[LEN001]: {message} (fn `oversized`)")),
        "{stderr}"
    );
}

/// `--include LEN001` whitelists the code alone: the finding fires and
/// every other code stays off.
#[test]
fn len001_should_run_alone_when_included_by_code() {
    let source = format!(
        "pub fn undocumented() {{}}\n{}",
        fn_with_body_lines(MethodLengthConfig::default().max_lines + 1)
    );

    let output = run(&source, "source.rs", "{}", &["--include", "LEN001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("LEN001").count(), 1, "{stderr}");
    assert!(
        !stderr.contains("DOC001"),
        "the code-only whitelist must suppress every other code: {stderr}"
    );
}

// ── helpers ───────────────────────────────────────────────────────

/// A `fn oversized()` body of exactly `count` counted lines; the module
/// doc keeps broad-selection runs free of unrelated findings.
fn fn_with_body_lines(count: usize) -> String {
    let body: String = (0..count)
        .map(|i| format!("    let _v{i} = {i};\n"))
        .collect();
    format!("//! Loads merged parts.\nfn oversized() {{\n{body}}}\n")
}

/// Run an isolated file with explicit configuration and automatic fixture
/// cleanup.
fn run(source: &str, relative_path: &str, config: &str, args: &[&str]) -> Output {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    let config_path = dir.path().join(".rust-llm-tidy.yml");
    fs::write(&config_path, config).unwrap();

    Command::new(binary())
        .arg("--config")
        .arg(config_path)
        .args(args)
        .arg(path)
        .output()
        .unwrap()
}
