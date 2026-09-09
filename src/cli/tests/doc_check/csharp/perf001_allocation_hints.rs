//! C# built-in capacity reminders through the CLI fixture runner.

use super::{csharp_fixture_dir, run_csharp_fixture};
use crate::run_command;

#[test]
fn capacity_reminders_should_be_suppressed_when_symbols_are_excluded() {
    let path = csharp_fixture_dir().join("perf001_api_reminders.cs");

    let output = run_command(&["--include", "lints", "--exclude", "SYM"], &path);

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SYM"));
}

#[test]
fn capacity_reminders_should_ignore_shaped_constructions_and_text() {
    let (stderr, exit) = run_csharp_fixture("perf001_silent.cs");

    assert_eq!(exit, 0);
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn capacity_reminders_should_render_builtin_guidance() {
    let (stderr, exit) = run_csharp_fixture("perf001_api_reminders.cs");

    assert_eq!(exit, 0);
    assert_eq!(
        stderr
            .matches("reminder[SYM]: PERF001: API performance reminder:")
            .count(),
        3
    );
    assert!(stderr.contains("`new List<T>()` starts with zero capacity."));
    assert!(stderr.contains("`new Dictionary<K, V>()` starts with zero capacity."));
    assert!(stderr.contains("`new StringBuilder()` starts without a chosen capacity."));
    assert!(stderr.contains("- If the expected element count is known, use the `List` constructor that accepts a capacity."));
}
