//! TEXT003 over-budget Rust doc sentences.
//!
//! The test runs the built CLI binary on a fixture in
//! `tests/fixtures/doc/rust/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::run_rust_fixture;

/// Rust doc sentences over the word budget warn with TEXT003 at their
/// start lines.
///
/// - The wrapped sentence reports where its first word sits.
/// - The 25-word sentence stays silent.
/// - Warnings keep the exit code at 0.
#[test]
fn rs_long_doc_sentences_warn_text003() {
    let (stderr, exit) =
        run_rust_fixture("text-003_sentence_budgets.rs", "TEXT001,TEXT002,TEXT003");

    assert_eq!(exit, 0, "TEXT003 warnings must not fail the run:\n{stderr}");
    assert!(
        stderr.contains(":6: warning[TEXT003]"),
        "TEXT003 must report at the over-limit sentence's start line:\n{stderr}"
    );
    assert!(
        stderr.contains(":12: warning[TEXT003]"),
        "TEXT003 must report at the wrapped sentence's start line:\n{stderr}"
    );
    assert!(
        !stderr.contains(":1: warning[TEXT003]"),
        "the 25-word sentence is at the limit, not over it:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT003").count(),
        2,
        "the over-limit and wrapped sentences only, never the 25-word one:\n{stderr}"
    );
    assert!(
        !stderr.contains("TEXT001") && !stderr.contains("TEXT002"),
        "the fixture stays inside the paragraph and line budgets:\n{stderr}"
    );
}
