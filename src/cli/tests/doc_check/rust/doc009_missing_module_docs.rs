//! DOC009 module files without top-level docs over the Rust fixtures.
//!
//! Every test runs `--include DOC009` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::{run_command, rust_fixture_dir};

/// `doc009_missing_module_docs.rs` errors when a module file carries
/// top-level items but no `//!` module-doc preamble.
#[test]
fn doc009_missing_module_docs() {
    let path = rust_fixture_dir().join("doc009_missing_module_docs.rs");
    let output = run_command(&["--include", "DOC009"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code().unwrap_or(-1),
        0,
        "a module file without `//!` docs should fail"
    );
    assert!(
        stderr.contains(":1: error[DOC009]: module file is missing `//!` module docs (file)"),
        "DOC009 must render its pinned line-1 diagnostic:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC009").count(),
        1,
        "one finding per file, never per item:\n{stderr}"
    );
}
