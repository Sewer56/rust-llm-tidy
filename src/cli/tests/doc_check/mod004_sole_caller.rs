//! MOD004 sole-caller module nesting across a two-crate workspace.
//!
//! Whole-workspace runs must index every crate owning an input, not
//! just the first sorted one. The fixture's `beta` crate sorts after
//! `alpha`, so its hint only appears with per-crate indexing.

use crate::common::binary;
use crate::manifest_dir;
use std::process::Command;

/// A workspace run reports each crate's sole-caller hint, including
/// the crate that sorts last.
#[test]
fn mod004_should_flag_every_crate_when_run_spans_workspace() {
    let root = manifest_dir().join("tests/fixtures/doc/rust/mod004_workspace");

    let output = Command::new(binary())
        .args(["--no-config", "--include", "MOD004"])
        .arg(&root)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "hints never fail the run:\n{stderr}"
    );
    assert_eq!(stderr.matches("hint[MOD004]").count(), 2, "{stderr}");
    assert!(
        stderr.contains(
            "alpha/src/load.rs:2: hint[MOD004]: module `crate::xbe` is referenced only by `crate::load` (1 reference)."
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "beta/src/load.rs:2: hint[MOD004]: module `crate::codec` is referenced only by `crate::load` (1 reference)."
        ),
        "{stderr}"
    );
}
