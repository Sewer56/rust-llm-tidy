//! Rule: TEST002 - test functions need a summary comment.
//!
//! A test function should open with a short comment above its attribute block
//! describing what it verifies and why it matters. A `///` doc run or a plain
//! `//` block directly above the attributes satisfies the rule; presence only
//! is checked, never wording.
//!
//! Expected diagnostics:
//! - TEST002 on `missing_summary` (no comment above the attributes)
//! - TEST002 on `blank_line_before_attributes` (comment separated by a blank line)
//! - TEST002 on `comment_below_attributes` (comment below the attribute block)
//! - TEST002 on `ignored_test_without_summary` (`#[ignore]` grants no exemption)
//!
//! Not flagged (should pass):
//! - `doc_comment_summary` (`///` above the attributes)
//! - `plain_comment_summary` (`//` above the attributes)
//! - `not_a_test` (no test marker)

#[test]
fn missing_summary() {}

// This comment is separated from the attribute block by a blank line.

#[test]
fn blank_line_before_attributes() {}

#[test]
// A comment below the attributes does not count as a summary.
fn comment_below_attributes() {}

#[ignore]
#[test]
fn ignored_test_without_summary() {}

/// Verifies the documented path.
#[test]
fn doc_comment_summary() {}

// Verifies the plain-comment path.
#[test]
fn plain_comment_summary() {}

fn not_a_test() {}
