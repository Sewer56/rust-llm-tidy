//! DOC002 missing `<exception>` tags over the C# fixtures.
//!
//! DOC002 resolves throwing members across calls and files. Fixture
//! tests run through the runner in `mod.rs`; cross-file tests spawn the
//! CLI binary directly on temp files.

use super::run_csharp_fixture;
use crate::common::binary;
use crate::{assert_has_diagnostic, manifest_dir, run_command, temp_file};
use std::fs;
use std::process::Command;

/// DOC002 recursion follows same-file throwers transitively.
///
/// - A caller without its own `throw` is flagged for calling a same-file thrower.
/// - The private thrower, framework calls, and tagged callers stay silent.
/// - Findings keep document order and error severity.
#[test]
fn csharp_doc002_errors_on_indirect_throwers() {
    let (stderr, exit) = run_csharp_fixture("doc002_indirect_exception.cs");

    assert_ne!(exit, 0, "DOC002 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains("error[DOC002]"),
        "DOC002 carries error severity:\n{stderr}"
    );
    assert_has_diagnostic(&stderr, "DOC002", Some("Load"));
    assert_has_diagnostic(&stderr, "DOC002", Some("LoadTwice"));
    assert!(
        !stderr.lines().any(|line| line.contains("[DOC002]")
            && (line.contains("`Validate`")
                || line.contains("`Parse`")
                || line.contains("`LoadGuarded`"))),
        "private throwers, framework calls, and tagged callers pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC002").count(),
        2,
        "expected exactly 2 DOC002 findings:\n{stderr}"
    );
    let direct = stderr.find("(fn `Load`)").expect("Load must be named");
    let transitive = stderr
        .find("(fn `LoadTwice`)")
        .expect("LoadTwice must be named");
    assert!(
        direct < transitive,
        "findings stay in document order:\n{stderr}"
    );
}

/// DOC002 errors on the documented non-private thrower without an
/// `<exception>` tag; the tagged and private throwers pass.
#[test]
fn csharp_doc002_errors_on_untagged_throwers() {
    let (stderr, exit) = run_csharp_fixture("doc002_missing_exception.cs");

    assert_ne!(exit, 0, "DOC002 errors must fail the run:\n{stderr}");
    assert_has_diagnostic(&stderr, "DOC002", Some("Untagged"));
    assert!(
        stderr.contains("error[DOC002]"),
        "DOC002 carries error severity:\n{stderr}"
    );
    assert!(
        !stderr.lines().any(|line| line.contains("[DOC002]")
            && (line.contains("`Tagged`") || line.contains("`Hidden`"))),
        "tagged and private throwers pass:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC002").count(),
        1,
        "expected exactly 1 DOC002 finding:\n{stderr}"
    );
}

/// A loose caller has no foreign facts; vague tags warn without failing a
/// paired invocation.
#[test]
fn csharp_doc002_should_degrade_for_loose_files_and_keep_doc003_warning_exit() {
    let caller = temp_file("cs");
    let helper = temp_file("cs");
    fs::write(
        &caller,
        "class A {\n/// <summary>Loads a value.</summary>\npublic void Load() { T.Helper(); }\n}",
    )
    .unwrap();
    fs::write(&helper, "class T { void Helper() { throw new E(); } }").unwrap();

    let loose = run_command(&["--include", "lints"], &caller);
    fs::write(
        &caller,
        "class A {\n/// <exception>Failure.</exception>\npublic void Load() { T.Helper(); }\n}",
    )
    .unwrap();
    let paired = Command::new(binary())
        .args(["--no-config", "--include", "lints"])
        .arg(&caller)
        .arg(&helper)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&paired.stderr);
    fs::remove_file(caller).unwrap();
    fs::remove_file(helper).unwrap();

    assert!(loose.status.success());
    assert!(loose.stderr.is_empty());
    assert!(paired.status.success(), "{stderr}");
    assert_eq!(stderr.matches("warning[DOC003]").count(), 1, "{stderr}");
    assert_eq!(stderr.matches("warning[").count(), 1, "{stderr}");
    assert!(!stderr.contains("error["), "{stderr}");
}

/// Explicit file pairs and project-scoped single inputs report the same
/// cross-file error.
#[test]
fn csharp_doc002_should_find_project_throwers_from_single_or_multiple_inputs() {
    let root = manifest_dir().join("tests/fixtures/doc/csharp/doc002_cross_file");
    let caller = root.join("caller/Caller.cs");
    let thrower = root.join("thrower/Thrower.cs");

    for multiple in [false, true] {
        let mut command = Command::new(binary());
        command
            .args(["--no-config", "--include", "lints"])
            .arg(&caller);
        if multiple {
            command.arg(&thrower);
        }

        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(!output.status.success(), "{stderr}");
        assert_eq!(stderr.matches("error[DOC002]").count(), 1, "{stderr}");
        assert_eq!(stderr.matches("(fn `Load`)").count(), 1, "{stderr}");
        assert_eq!(stderr.matches("error[").count(), 1, "{stderr}");
        assert!(!stderr.contains("warning["), "{stderr}");
    }
}

/// Real member movement preserves the same current-source lint records as a
/// fresh lint pass.
#[test]
fn csharp_doc002_should_refresh_diagnostic_positions_after_reorder() {
    let caller = temp_file("cs");
    let helper = temp_file("cs");
    let source = "class A\n{\n    /// <summary>Loads first.</summary>\n    public void First() { T.Helper(); }\n    /// <summary>Loads second.</summary>\n    public void Second() { First(); }\n}\n";
    fs::write(&caller, source).unwrap();
    fs::write(&helper, "class T { void Helper() { throw new E(); } }").unwrap();
    let run = |include| {
        Command::new(binary())
            .args(["--no-config", "--output-mode", "json", "--include", include])
            .arg(&caller)
            .arg(&helper)
            .output()
            .unwrap()
    };

    let combined = Command::new(binary())
        .args([
            "--no-config",
            "--output-mode",
            "json",
            "--include",
            "reorder",
            "--include",
            "lints",
        ])
        .arg(&caller)
        .arg(&helper)
        .output()
        .unwrap();
    let current = fs::read_to_string(&caller).unwrap();
    let fresh = run("lints");
    let diagnostics = |output: &[u8]| {
        let records: Vec<serde_json::Value> = serde_json::from_slice(output).unwrap();
        records
            .into_iter()
            .filter(|record| record["code"] == "DOC002")
            .collect::<Vec<_>>()
    };
    let combined_records = diagnostics(&combined.stdout);
    let fresh_records = diagnostics(&fresh.stdout);
    fs::remove_file(&caller).unwrap();
    fs::remove_file(&helper).unwrap();

    assert_ne!(current, source);
    assert!(current.find("void Second").unwrap() < current.find("void First").unwrap());
    assert_eq!(combined.status.code(), fresh.status.code());
    assert!(!combined.status.success());
    assert_eq!(combined_records.len(), 2);
    assert_eq!(combined_records, fresh_records);
    assert_eq!(combined_records[0]["line"], 3);
    assert_eq!(combined_records[1]["line"], 5);
}
