//! LEN001 end-to-end acceptance through the real CLI pipeline.
//!
//! Covers the default firing point, a configured threshold override,
//! the rendered split advice, and `--include`/`--exclude` gating, with
//! a config-file runner mirroring the MOD001 suite.

use crate::common::binary;
use rstest::rstest;
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
fn len001_should_follow_cli_rule_selection(#[case] args: &[&str], #[case] warns: bool) {
    let output = run(&fn_with_body_lines(76), "source.rs", "{}", args);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("warning[LEN001]"), warns, "{stderr}");
}

/// Config selection gates LEN001 like any other rule.
#[rstest]
#[case::included("include:\n  - rules: [LEN001]\n", true)]
#[case::excluded("exclude:\n  - rules: [LEN001]\n", false)]
#[case::excluded_group("exclude:\n  - rules: [lints]\n", false)]
fn len001_should_follow_config_rule_selection(#[case] selection: &str, #[case] warns: bool) {
    let output = run(
        &fn_with_body_lines(76),
        "source.rs",
        selection,
        &["--dry-run"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("warning[LEN001]"), warns, "{stderr}");
}

/// A configured `method_length.max_lines` moves the firing point: 4
/// counted lines warn under a 3-line budget and stay silent under the
/// 75 default.
#[test]
fn len001_should_follow_the_configured_max_lines_threshold() {
    let source = fn_with_body_lines(4);

    let configured = run(
        &source,
        "source.rs",
        "method_length:\n  max_lines: 3\n",
        &["--include", "LEN001"],
    );
    let configured_stderr = String::from_utf8_lossy(&configured.stderr);
    assert!(configured.status.success(), "{configured_stderr}");
    assert!(
        configured_stderr.contains("fn `oversized` has 4 body lines"),
        "the override must take effect: {configured_stderr}"
    );
    assert!(
        configured_stderr.contains("over the 3-line budget"),
        "the warning must state the configured budget: {configured_stderr}"
    );

    let default = run(&source, "source.rs", "{}", &["--include", "LEN001"]);
    let default_stderr = String::from_utf8_lossy(&default.stderr);
    assert!(
        default.status.success() && !default_stderr.contains("LEN001"),
        "4 lines stay under the 75 default: {default_stderr}"
    );
}

/// The rendered warning states the facts and carries the split advice.
#[test]
fn len001_should_render_the_full_split_advice() {
    let output = run(
        &fn_with_body_lines(4),
        "source.rs",
        "method_length:\n  max_lines: 3\n",
        &["--include", "LEN001"],
    );
    let expected = indoc::formatdoc! {"
        warning[LEN001]: fn `oversized` has 4 body lines (blank and comment-only lines excluded),
        over the 3-line budget (method_length.max_lines).
        - Long functions are hard to follow: readers must hold the whole
          control flow and every local in mind at once.
        - Split the body into smaller sub-functions or inner functions, each
          named for what it does.
        - Keep the outer function short enough to read as an overview of
          the flow. (fn `oversized`)"};

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(&expected), "{stderr}");
}

/// `--include LEN001` whitelists the code alone: the finding fires and
/// every other code stays off.
#[test]
fn len001_should_run_alone_when_included_by_code() {
    let source = format!("pub fn undocumented() {{}}\n{}", fn_with_body_lines(76));

    let output = run(&source, "source.rs", "{}", &["--include", "LEN001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("LEN001").count(), 1, "{stderr}");
    assert!(
        !stderr.contains("DOC001"),
        "the code-only whitelist must suppress every other code: {stderr}"
    );
}

/// Exactly at the default budget: silent, because strictly greater fires.
#[test]
fn len001_should_stay_silent_at_exactly_the_default_budget() {
    let output = run(
        &fn_with_body_lines(75),
        "source.rs",
        "{}",
        &["--include", "LEN001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("LEN001"), "{stderr}");
}

/// Over the default budget: one warning naming the fn, its measured
/// count, and the budget; warnings never fail the run.
#[test]
fn len001_should_warn_when_body_lines_exceed_the_default_budget() {
    let output = run(
        &fn_with_body_lines(76),
        "source.rs",
        "{}",
        &["--include", "LEN001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("warning[LEN001]").count(), 1, "{stderr}");
    assert!(stderr.contains(":2: warning[LEN001]"), "{stderr}");
    assert!(
        stderr.contains("fn `oversized` has 76 body lines"),
        "the warning must state the measured count: {stderr}"
    );
    assert!(
        stderr.contains("over the 75-line budget (method_length.max_lines)"),
        "the warning must state the default budget: {stderr}"
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
