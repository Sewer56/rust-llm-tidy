//! C# PERF001 tests: API reminders over the C# parse.
//!
//! The fixtures live in `tests/fixtures/doc/csharp/perf001_*.cs`; the
//! runner helpers live in `mod.rs`.

use super::{csharp_fixture_dir, run_csharp_fixture};
use crate::{binary, run_command, temp_dir};
use std::fs;
use std::process::Command;

/// A source exercising one built-in (`List::new`) and one opt-in call
/// (`ToUpper`), for configured-list tests.
const REMINDED_SOURCE: &str = concat!(
    "class Reminded\n",
    "{\n",
    "    void Shout(string prefix, int[] rows)\n",
    "    {\n",
    "        foreach (var row in rows)\n",
    "        {\n",
    "            var label = prefix.ToUpper();\n",
    "            var values = new List<int>();\n",
    "            System.Console.WriteLine(label + values.Count + row);\n",
    "        }\n",
    "    }\n",
    "}\n",
);

/// `perf001_api_reminders.cs` hints on every built-in container
/// construction, without failing the run.
#[test]
fn csharp_perf001_api_reminders_emit_builtin_hints() {
    let (stderr, exit) = run_csharp_fixture("perf001_api_reminders.cs");

    assert_eq!(exit, 0, "PERF001 hints must not fail the run");
    assert!(
        stderr.contains("reminder[PERF001]: `new List<T>()` starts with zero capacity."),
        "the List creation must carry the List reminder:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: `new Dictionary<K, V>()` starts with zero capacity."),
        "the Dictionary creation must carry the Dictionary reminder:\n{stderr}"
    );
    assert!(
        stderr
            .contains("reminder[PERF001]: `new StringBuilder()` starts without a chosen capacity."),
        "the StringBuilder creation must carry the StringBuilder reminder:\n{stderr}"
    );
    assert!(
        stderr
            .contains("- If the expected element count is known, use the `List` constructor that accepts a capacity."),
        "the List reminder must suggest the capacity constructor:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("PERF001").count(),
        3,
        "expected exactly 3 PERF001 findings:\n{stderr}"
    );
}

/// `perf_hints: []` plus extras runs the extras only.
#[test]
fn csharp_perf001_empty_replacement_leaves_extras_only() {
    let (stderr, exit) = run_with_config(concat!(
        "perf_hints: []\n",
        "extra_perf_hints:\n",
        "  - pattern: ToUpper\n",
        "    message: consider an invariant-culture overload\n",
    ));

    assert_eq!(exit, 0, "PERF001 hints must not fail the run");
    assert!(
        !stderr.contains("the `List` constructor that accepts a capacity"),
        "the cleared built-ins must not fire:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("PERF001").count(),
        1,
        "exactly the extra reminder fires:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: consider an invariant-culture overload"),
        "the extra reminder must fire:\n{stderr}"
    );
}

/// `extra_perf_hints` appends: the built-in List reminder and the
/// opt-in `ToUpper` reminder both fire.
#[test]
fn csharp_perf001_extra_hints_apply_after_builtins() {
    let (stderr, exit) = run_with_config(concat!(
        "extra_perf_hints:\n",
        "  - pattern: ToUpper\n",
        "    message: consider an invariant-culture overload\n",
    ));

    assert_eq!(exit, 0, "PERF001 hints must not fail the run");
    assert!(
        stderr.contains("reminder[PERF001]: `new List<T>()` starts with zero capacity."),
        "the built-in reminder must keep firing:\n{stderr}"
    );
    assert!(
        stderr.contains("reminder[PERF001]: consider an invariant-culture overload"),
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
fn csharp_perf001_replacement_list_suppresses_builtins() {
    let (stderr, exit) = run_with_config(concat!(
        "perf_hints:\n",
        "  - pattern: ToUpper\n",
        "    message: consider an invariant-culture overload\n",
    ));

    assert_eq!(exit, 0, "the custom hint must not fail the run");
    assert!(
        stderr.contains("reminder[PERF001]: consider an invariant-culture overload"),
        "the configured reminder must fire:\n{stderr}"
    );
    assert!(
        !stderr.contains("the `List` constructor that accepts a capacity"),
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
fn csharp_perf001_replacement_with_extras_equals_concatenated_replacement() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".git"), "").unwrap();
    fs::write(dir.join("Reminded.cs"), REMINDED_SOURCE).unwrap();
    let config_path = dir.join(".rust-llm-tidy.yml");

    fs::write(
        &config_path,
        concat!(
            "perf_hints:\n",
            "  - pattern: List::new\n    message: base reminder\n",
            "extra_perf_hints:\n",
            "  - pattern: ToUpper\n    message: extra reminder\n",
        ),
    )
    .unwrap();
    let split = Command::new(binary())
        .current_dir(&dir)
        .args(["--include", "lints", "--lint-scope", "all"])
        .arg("Reminded.cs")
        .output()
        .unwrap();

    fs::write(
        &config_path,
        concat!(
            "perf_hints:\n",
            "  - pattern: List::new\n    message: base reminder\n",
            "  - pattern: ToUpper\n    message: extra reminder\n",
        ),
    )
    .unwrap();
    let joined = Command::new(binary())
        .current_dir(&dir)
        .args(["--include", "lints", "--lint-scope", "all"])
        .arg("Reminded.cs")
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

/// `perf001_silent.cs` stays silent: shaped constructions and mentions
/// in comments or strings never fire.
#[test]
fn csharp_perf001_stays_silent_on_shaped_constructions_and_mentions() {
    let (stderr, exit) = run_csharp_fixture("perf001_silent.cs");

    assert_eq!(exit, 0, "the negative fixture should pass");
    assert!(
        stderr.is_empty(),
        "non-matching constructions must stay silent:\n{stderr}"
    );
}

/// `--exclude PERF001` suppresses the C# reminders.
#[test]
fn csharp_perf001_suppressed_by_exclude() {
    let path = csharp_fixture_dir().join("perf001_api_reminders.cs");
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
    fs::write(dir.join("Reminded.cs"), REMINDED_SOURCE).unwrap();

    let output = Command::new(binary())
        .current_dir(&dir)
        .args(["--include", "lints", "--lint-scope", "all"])
        .arg("Reminded.cs")
        .output()
        .unwrap();
    let rendered = (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    );
    let _ = fs::remove_dir_all(dir);
    rendered
}
