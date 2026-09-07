//! TEXT004 three-sentence Rust doc openers.
//!
//! Every test runs the built CLI binary on a Rust fixture and asserts on
//! its exit code and stderr diagnostics. The shared runner helper lives
//! in `mod.rs`.

use super::run_rust_fixture;
use crate::{run_command, rust_fixture_dir};

/// `--include TEXT004` selects the rule by code: the whitelist accepts
/// the name and the fixture reports exactly its two opener findings.
#[test]
fn rs_text004_selectable_by_include_code() {
    let path = rust_fixture_dir().join("text-004_header_openers.rs");
    let output = run_command(&["--include", "TEXT004"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the code-only whitelist must accept TEXT004 and exit 0: {stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT004").count(),
        2,
        "the code whitelist must run the rule:\n{stderr}"
    );
}

/// Rust doc openers with three or more sentences warn with TEXT004 at
/// each opener's first line.
///
/// - The module doc's heading-following paragraph and the item doc's
///   first paragraph fire.
/// - The one-sentence module opener and one-sentence item opener stay silent.
/// - The two-sentence opener stays silent at the limit.
/// - Warnings keep the exit code at 0.
#[test]
fn rs_three_sentence_doc_openers_warn_text004() {
    let (stderr, exit) = run_rust_fixture("text-004_header_openers.rs");

    assert_eq!(exit, 0, "TEXT004 warnings must not fail the run:\n{stderr}");
    assert!(
        stderr.contains(":5: warning[TEXT004]"),
        "TEXT004 must report at the heading opener's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":7: warning[TEXT004]"),
        "TEXT004 must report at the item opener's first line:\n{stderr}"
    );
    assert!(
        !stderr.contains(":1: warning[TEXT004]"),
        "the one-sentence module opener stays silent:\n{stderr}"
    );
    assert!(
        !stderr.contains(":10: warning[TEXT004]"),
        "the two-sentence opener is at the limit, not over it:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT004").count(),
        2,
        "the three-sentence heading and item openers only, never the clean ones:\n{stderr}"
    );
    assert!(
        !stderr.contains("TEXT001") && !stderr.contains("TEXT002"),
        "the fixture stays inside the paragraph and line budgets:\n{stderr}"
    );
}
