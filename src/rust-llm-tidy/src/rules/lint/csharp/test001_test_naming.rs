//! `TEST001` - test-method naming.

use super::Declaration;
use crate::languages::csharp::parse::has_test_marker;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_TEST_NAMING;

/// `TEST001` - test methods should use behavioral names.
///
/// Fires on `TestMethod`/`Test`/`Fact`/`Theory`-marked methods whose
/// names use a discouraged pattern: the bare `test` name, `test_*` /
/// `case_*` prefixes, and `test` immediately followed by digits.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    if has_test_marker(decl.node, decl.source)
        && let Some(name) = decl.name.as_deref()
        && is_bad_test_name(name)
    {
        return vec![decl.diagnostic(
            Severity::Warning,
            CODE_TEST_NAMING,
            format!(
                "test method `{name}` should use a behavioral name \
                 (subject_should_expectation_when_condition), not a `test_*` or `case_*` prefix"
            ),
        )];
    }
    Vec::new()
}

/// True when `name` uses a discouraged test-naming pattern.
///
/// ASCII case-insensitive counterpart of the Rust rule: the bare `test`
/// name, `test_*`/`case_*` prefixes, and `test` immediately followed by
/// digits; behavioral names like `ShouldReturnNullWhenMissing` pass.
fn is_bad_test_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "test" || lower.starts_with("test_") || lower.starts_with("case_") {
        return true;
    }
    lower
        .strip_prefix("test")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}
