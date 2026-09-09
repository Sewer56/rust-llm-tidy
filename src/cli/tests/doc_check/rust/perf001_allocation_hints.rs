//! Rust built-in capacity reminders through the CLI fixture runner.

use super::run_rust_fixture;
use crate::{run_command, rust_fixture_dir};

#[test]
fn capacity_reminders_should_be_suppressed_when_symbols_are_excluded() {
    let path = rust_fixture_dir().join("perf001_api_reminders.rs");

    let output = run_command(&["--include", "lints", "--exclude", "SYM"], &path);

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SYM"));
}

#[test]
fn capacity_reminders_should_ignore_shaped_calls_and_text() {
    let (stderr, exit) = run_rust_fixture("perf001_silent.rs", "lints");

    assert_eq!(exit, 0);
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn capacity_reminders_should_render_builtin_guidance() {
    let (stderr, exit) = run_rust_fixture("perf001_api_reminders.rs", "SYM");

    assert_eq!(exit, 0);
    assert_eq!(
        stderr
            .matches("reminder[SYM]: PERF001: API performance reminder:")
            .count(),
        3
    );
    assert!(stderr.contains("`Vec::new()` starts with zero capacity."));
    assert!(stderr.contains("`String::new()` starts with zero capacity."));
    assert!(stderr.contains("`HashMap::new()` starts with zero capacity."));
    assert!(
        stderr
            .contains("- If the expected element count is known, use `Vec::with_capacity(count)`.")
    );
}

#[test]
fn capacity_reminders_should_serialize_symbol_identity_and_performance_title() {
    let path = rust_fixture_dir().join("perf001_api_reminders.rs");

    let output = run_command(&["--include", "SYM", "--output-mode", "json"], &path);
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success());
    assert_eq!(records.len(), 3);
    for record in records {
        assert_eq!(record["code"], "SYM");
        assert_eq!(record["severity"], "reminder");
        assert_eq!(record["title"], "PERF001: API performance reminder");
    }
}
