//! Rule: TEST001 - test functions should use behavioral names.
//!
//! A `#[test]` function should describe the behavior it verifies
//! (`subject_should_expectation_when_condition`) rather than reuse a redundant
//! `test_*` or `case_*` prefix the enclosing test module already implies.
//!
//! Expected diagnostics:
//! - TEST001 on `test_foo` (redundant `test_` prefix)
//! - TEST001 on `test1` (`test` + digits)
//! - TEST001 on `case_1` (redundant `case_` prefix)
//!
//! Not flagged (should pass):
//! - `should_pass_when_valid` (behavioral name)

#[test]
fn test_foo() {}

#[test]
fn test1() {}

#[test]
fn case_1() {}

#[test]
fn should_pass_when_valid() {}
