//! `fix --include fences` dry-run reporting.
//!
//! The shared runner helpers live in `mod.rs`.

use super::{fixture_dir, run_command};

/// `fix --dry-run` on `fence_md_before.md` reports a change record on stderr.
#[test]
fn fix_fence_md_dry_run_reports_change() {
    let before = fixture_dir().join("fence_md_before.md");
    let output = run_command(&["--include", "fences", "--dry-run"], &before);

    assert!(
        !output.status.success(),
        "fix --dry-run must fail for proposed changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("success[FIX]"),
        "dry-run must report a fix change line on stderr: {stderr}"
    );
}
