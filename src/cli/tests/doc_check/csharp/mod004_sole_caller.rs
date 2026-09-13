//! MOD004 sole-caller namespace placement over cross-file C# fixtures.
//!
//! MOD004 measures namespace references across the project closure.
//! Cross-file tests spawn the CLI binary directly on the fixture
//! projects, mirroring the DOC002 pattern.

use crate::common::binary;
use crate::manifest_dir;
use std::process::Command;

/// Project-scope and explicit-pair inputs report the same sole-caller
/// hint, anchored at the caller's file.
#[test]
fn csharp_mod004_should_flag_sole_caller_namespace_from_single_or_multiple_inputs() {
    let root = manifest_dir().join("tests/fixtures/doc/csharp/mod004_cross_file");
    let caller = root.join("caller/Caller.cs");
    let lib = root.join("lib/Lib.cs");

    for multiple in [false, true] {
        let mut command = Command::new(binary());
        command
            .args(["--no-config", "--include", "MOD004"])
            .arg(&caller);
        if multiple {
            command.arg(&lib);
        }

        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "hints never fail the run:\n{stderr}"
        );
        assert_eq!(stderr.matches("hint[MOD004]").count(), 1, "{stderr}");
        assert!(
            stderr.contains("Caller.cs:7: hint[MOD004]: namespace `App.Core` is referenced only by `App.Run` (1 reference)."),
            "{stderr}"
        );
        assert!(stderr.contains("(namespace `Core`)"), "{stderr}");
    }
}

/// Excluding MOD004 suppresses the hint without touching the other
/// lints' silence on the fixture.
#[test]
fn csharp_mod004_should_stay_silent_when_excluded() {
    let root = manifest_dir().join("tests/fixtures/doc/csharp/mod004_cross_file");
    let caller = root.join("caller/Caller.cs");

    let mut command = Command::new(binary());
    command.args(["--no-config", "--include", "MOD004", "--exclude", "MOD004"]);
    command.arg(&caller);

    let output = command.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("MOD004"), "{stderr}");
}
