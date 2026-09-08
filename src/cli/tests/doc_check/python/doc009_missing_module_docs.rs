//! Missing Python module-docstring diagnostics over the Python fixtures.
//!
//! Every test runs the built CLI binary and asserts on its exit code and
//! stderr diagnostics.

use crate::{python_fixture_dir, run_command};

/// `doc009_missing_docstring.py` errors when a module carries top-level
/// content but no module docstring.
#[test]
fn doc009_missing_docstring() {
    let path = python_fixture_dir().join("doc009_missing_docstring.py");
    let output = run_command(&["--include", "DOC009"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code().unwrap_or(-1),
        0,
        "a module without a docstring should fail"
    );
    assert!(
        stderr.contains(":1: error[DOC009]: module file is missing a module docstring (file)"),
        "DOC009 must render its pinned line-1 diagnostic:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC009").count(),
        1,
        "one finding per module, never per statement:\n{stderr}"
    );
}
