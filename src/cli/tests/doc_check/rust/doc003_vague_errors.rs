//! DOC003 vague `# Errors` sections over the Rust fixtures.
//!
//! Every test runs `--include DOC003` on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;
use crate::assert_has_diagnostic;

/// The bracket-link variant (`[Error::NotFound]`) passes DOC003.
#[test]
fn doc003_bracket_link_passes() {
    let (stderr, _exit) = run_rust_fixture("doc003_vague_errors.rs", "DOC003");
    assert!(
        !stderr.contains("specific_load_bracket"),
        "fn with variant link should not be flagged:\n{stderr}"
    );
}

/// The path-qualified variant (`Error::Timeout` via `::`) passes DOC003.
#[test]
fn doc003_path_qualified_passes() {
    let (stderr, _exit) = run_rust_fixture("doc003_vague_errors.rs", "DOC003");
    assert!(
        !stderr.contains("specific_load_path"),
        "fn with path-qualified variant should not be flagged:\n{stderr}"
    );
}

/// `doc003_vague_errors.rs` warns on `# Errors` sections that name no variant.
#[test]
fn doc003_vague_errors() {
    let (stderr, exit) = run_rust_fixture("doc003_vague_errors.rs", "DOC003");

    // DOC003 is a warning - it should not fail the run (only errors fail).
    assert_eq!(exit, 0, "DOC003 warnings should not fail the run");

    assert_has_diagnostic(&stderr, "DOC003", Some("vague_load"));

    let doc003_count = stderr.matches("DOC003").count();
    assert_eq!(
        doc003_count, 1,
        "expected exactly 1 DOC003 finding, got {doc003_count}:\n{stderr}"
    );
}
