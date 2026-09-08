//! PERF001 API-reminder tests over the `.rs` fixtures.
//!
//! Every configured reminder names an invoked API; a match emits the
//! entry's message at reminder severity. `perf_hints` replaces the
//! built-ins; `extra_perf_hints` applies after them. The shared runner
//! helpers live in `mod.rs`.

use super::run_rust_fixture;
use crate::{binary, run_command, rust_fixture_dir, temp_dir};
use std::fs;
use std::process::Command;

/// A source exercising one built-in (`Vec::new`) and one opt-in call
/// (`to_uppercase`), for configured-list tests.
const REMINDED_SOURCE: &str = concat!(
    "//! Exercises the configured PERF001 reminder lists.\n",
    "fn shout(prefix: &str, rows: usize) {\n",
    "    for row in 0..rows {\n",
    "        let label = prefix.to_uppercase();\n",
    "        let mut out = Vec::new();\n",
    "        println!(\"{label}{row}{}\", out.len());\n",
    "    }\n",
    "}\n",
);

/// `perf001_api_reminders.rs` hints on every built-in container
/// constructor, without failing the run.
#[test]
fn perf001_api_reminders_emit_builtin_hints() {
    let (stderr, exit) = run_rust_fixture("perf001_api_reminders.rs", "PERF001");

    assert_eq!(exit, 0, "PERF001 hints must not fail the run");
    assert!(
        stderr.contains("reminder[PERF001]: `Vec::new()` starts with zero capacity."),
        "the Vec::new call must carry the Vec reminder:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: `String::new()` starts with zero capacity."),
        "the String::new call must carry the String reminder:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: `HashMap::new()` starts with zero capacity."),
        "the HashMap::new call must carry the HashMap reminder:\n{stderr}"
    );
    assert!(
        stderr
            .contains("- If the expected element count is known, use `Vec::with_capacity(count)`."),
        "the Vec reminder must suggest the capacity alternative:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("PERF001").count(),
        3,
        "expected exactly 3 PERF001 findings:\n{stderr}"
    );
}

/// `perf_hints: []` disables every built-in reminder.
#[test]
fn perf001_empty_replacement_disables_builtins() {
    let (stderr, exit) = run_with_config("perf_hints: []\n");

    assert_eq!(exit, 0, "a disabled reminder list must not fail the run");
    assert!(
        !stderr.contains("PERF001"),
        "the built-ins must stay silent:\n{stderr}"
    );
}

/// `perf_hints: []` plus extras runs the extras only.
#[test]
fn perf001_empty_replacement_leaves_extras_only() {
    let (stderr, exit) = run_with_config(concat!(
        "perf_hints: []\n",
        "extra_perf_hints:\n",
        "  - pattern: to_uppercase\n",
        "    message: consider borrowing or hoisting the call\n",
    ));

    assert_eq!(exit, 0, "PERF001 hints must not fail the run");
    assert_eq!(
        stderr.matches("PERF001").count(),
        1,
        "exactly the extra reminder fires:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: consider borrowing or hoisting the call"),
        "the extra reminder must fire:\n{stderr}"
    );
    assert!(
        !stderr.contains("Vec::with_capacity"),
        "the cleared built-ins must not fire:\n{stderr}"
    );
}

/// `extra_perf_hints` appends: the built-in Vec reminder and the
/// opt-in `to_uppercase` reminder both fire.
#[test]
fn perf001_extra_hints_apply_after_builtins() {
    let (stderr, exit) = run_with_config(concat!(
        "extra_perf_hints:\n",
        "  - pattern: to_uppercase\n",
        "    message: consider borrowing or hoisting the call\n",
    ));

    assert_eq!(exit, 0, "PERF001 hints must not fail the run");
    assert!(
        stderr.contains("reminder[PERF001]: `Vec::new()` starts with zero capacity."),
        "the built-in reminder must keep firing:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: consider borrowing or hoisting the call"),
        "the extra reminder must fire:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("PERF001").count(),
        2,
        "exactly the built-in and extra reminders fire:\n{stderr}"
    );
}

/// A configured `perf_hints` list replaces the built-ins: only the
/// custom reminder fires.
#[test]
fn perf001_replacement_list_suppresses_builtins() {
    let (stderr, exit) = run_with_config(concat!(
        "perf_hints:\n",
        "  - pattern: to_uppercase\n",
        "    message: consider borrowing or hoisting the call\n",
    ));

    assert_eq!(exit, 0, "the custom hint must not fail the run");
    assert!(
        stderr.contains("reminder[PERF001]: consider borrowing or hoisting the call"),
        "the configured reminder must fire:\n{stderr}"
    );
    assert!(
        !stderr.contains("Vec::with_capacity"),
        "the replaced built-ins must not fire:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("PERF001").count(),
        1,
        "exactly the configured reminder fires:\n{stderr}"
    );
}

/// A replacement plus extras renders exactly the concatenated
/// replacement list over the same source and directory.
#[test]
fn perf001_replacement_with_extras_equals_concatenated_replacement() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".git"), "").unwrap();
    fs::write(dir.join("reminded.rs"), REMINDED_SOURCE).unwrap();
    let config_path = dir.join(".rust-llm-tidy.yml");

    fs::write(
        &config_path,
        concat!(
            "perf_hints:\n",
            "  - pattern: Vec::new\n    message: base reminder\n",
            "extra_perf_hints:\n",
            "  - pattern: to_uppercase\n    message: extra reminder\n",
        ),
    )
    .unwrap();
    let split = Command::new(binary())
        .current_dir(&dir)
        .args(["--include", "lints", "--lint-scope", "all"])
        .arg("reminded.rs")
        .output()
        .unwrap();

    fs::write(
        &config_path,
        concat!(
            "perf_hints:\n",
            "  - pattern: Vec::new\n    message: base reminder\n",
            "  - pattern: to_uppercase\n    message: extra reminder\n",
        ),
    )
    .unwrap();
    let joined = Command::new(binary())
        .current_dir(&dir)
        .args(["--include", "lints", "--lint-scope", "all"])
        .arg("reminded.rs")
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        String::from_utf8_lossy(&split.stderr),
        String::from_utf8_lossy(&joined.stderr),
        "both configs must render identical output"
    );
    assert_eq!(
        split.status.code(),
        joined.status.code(),
        "both configs must exit identically"
    );
    assert!(
        String::from_utf8_lossy(&split.stderr)
            .matches("PERF001")
            .count()
            == 2,
        "both reminders fire in each run"
    );
}

/// JSON output reports PERF001 findings at reminder severity.
#[test]
fn perf001_should_serialize_reminder_severity() {
    let path = rust_fixture_dir().join("perf001_api_reminders.rs");
    let output = run_command(&["--include", "PERF001", "--output-mode", "json"], &path);

    let records: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    let perf: Vec<&serde_json::Value> = records
        .iter()
        .filter(|record| record["code"] == "PERF001")
        .collect();
    assert_eq!(
        perf.len(),
        3,
        "every built-in reminder serializes:\n{records:?}"
    );
    for record in perf {
        assert_eq!(
            record["severity"], "reminder",
            "reminder severity serializes"
        );
        assert_eq!(
            record["title"], "API performance reminder",
            "the registry title follows the rule"
        );
    }
}

/// `perf001_silent.rs` stays silent: capacity-shaped calls and mentions
/// in comments or strings never fire.
#[test]
fn perf001_stays_silent_on_shaped_calls_and_mentions() {
    let (stderr, exit) = run_rust_fixture("perf001_silent.rs", "lints");

    assert_eq!(exit, 0, "the negative fixture should pass");
    assert!(
        stderr.is_empty(),
        "non-matching calls must stay silent:\n{stderr}"
    );
}

/// `--exclude PERF001` suppresses the reminders.
#[test]
fn perf001_suppressed_by_exclude() {
    let path = rust_fixture_dir().join("perf001_api_reminders.rs");
    let output = run_command(&["--include", "lints", "--exclude", "PERF001"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stderr.contains("PERF001"),
        "excluding PERF001 must suppress the findings:\n{stderr}"
    );
}

/// Write `config` and `REMINDED_SOURCE` into a fresh temp dir and run
/// `--include lints` there, returning (stderr, exit).
fn run_with_config(config: &str) -> (String, i32) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".git"), "").unwrap();
    fs::write(dir.join(".rust-llm-tidy.yml"), config).unwrap();
    fs::write(dir.join("reminded.rs"), REMINDED_SOURCE).unwrap();

    let output = Command::new(binary())
        .current_dir(&dir)
        .args(["--include", "lints", "--lint-scope", "all"])
        .arg("reminded.rs")
        .output()
        .unwrap();
    let rendered = (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    );
    let _ = fs::remove_dir_all(dir);
    rendered
}
