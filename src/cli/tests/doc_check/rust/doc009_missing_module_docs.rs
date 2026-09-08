//! DOC009 module files without top-level docs over the Rust fixtures.
//!
//! Every test runs `--include DOC009` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::{run_command, rust_fixture_dir};

/// Missing headers fail with purpose, shape, and module-root navigation guidance.
#[test]
fn doc009_should_explain_header_writing_when_module_docs_are_missing() {
    let path = rust_fixture_dir().join("doc009_missing_module_docs.rs");
    let expected = indoc::indoc! {"
        :1: error[DOC009]: module file is missing `//!` module docs.
        Fix: add `//!` docs before the first top-level item.

        Help readers unfamiliar with the codebase understand the module's purpose
        without reading its implementation.
        - Read the module and relevant callers; document only supported facts.
        - Start with one concise sentence explaining what the module does and why.
          Do not just restate its name. A simple module needs no more.
        - If more detail is useful, put it below the summary, separated by a blank
          doc line. Outline major responsibilities, entry points, or non-obvious
          constraints. Use bullets for multiple topics.
        - Link to item docs instead of repeating their details.
        - For a module root (`mod.rs`, or `foo.rs` with child modules), identify
          main entry points and relevant child-module responsibilities.
          This is header-writing guidance, not a request to move code. (file)"};

    let output = run_command(&["--include", "DOC009"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code().unwrap_or(-1),
        0,
        "a module file without `//!` docs should fail"
    );
    assert!(
        stderr.contains(expected),
        "DOC009 must render its pinned line-1 diagnostic:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("DOC009").count(),
        1,
        "one finding per file, never per item:\n{stderr}"
    );
}
