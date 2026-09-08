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
                "test method `{name}` uses a discouraged naming pattern.\n\n\
                 Why: Behavioral names help readers understand a test's claim without opening its body.\n\n\
                 Suggestions:\n\
                 - Rename it to describe the subject and expected behavior, adding a condition only when it matters.\n\
                 - Use `subject_should_expectation[_when_condition]` in the project's casing style."
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;
    use rstest::rstest;

    #[rstest]
    #[case::bare_test("Test")]
    #[case::test_prefix("Test_load")]
    #[case::case_prefix("Case_load")]
    #[case::numbered_test("Test42")]
    fn diagnostic_should_request_behavioral_name_in_project_casing(#[case] name: &str) {
        let source = format!("class CacheTests {{ [Fact] public void {name}() {{}} }}");
        let parsed = parse(&source).unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_TEST_NAMING)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            format!(
                "test method `{name}` uses a discouraged naming pattern.\n\n\
                 Why: Behavioral names help readers understand a test's claim without opening its body.\n\n\
                 Suggestions:\n\
                 - Rename it to describe the subject and expected behavior, adding a condition only when it matters.\n\
                 - Use `subject_should_expectation[_when_condition]` in the project's casing style."
            )
        );
    }
}
