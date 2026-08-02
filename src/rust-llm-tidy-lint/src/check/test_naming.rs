//! `TEST001` - test-function naming.
//!
//! [`test_naming`] fires on `#[test]` functions whose names use a discouraged
//! pattern (`test_*`, `case_*`, `test` + digits). Detection is delegated to the
//! module-private [`is_bad_test_name`] and [`is_test_plus_digits`] helpers.

use crate::check::CODE_TEST_NAMING;
use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::SourceItem;

/// `TEST001` - test functions should use behavioral names.
///
/// Fires on `#[test]` functions whose names use a discouraged pattern
/// (`test_*`, `case_*`, `test` + digits) instead of a behavioral claim shaped
/// `subject_should_expectation_when_condition`. Behavioral names describe the
/// behavior under test without the redundant `test_` prefix the test module
/// already provides.
pub fn test_naming(item: &SourceItem) -> Vec<Diagnostic> {
    if !item.is_test_fn() {
        return Vec::new();
    }
    let Some(name) = item.name() else {
        return Vec::new();
    };
    if !is_bad_test_name(name) {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_TEST_NAMING,
        message: format!(
            "test function `{name}` should use a behavioral name \
             (subject_should_expectation_when_condition), not a `test_*` or `case_*` prefix"
        ),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: Some(name.to_string()),
    }]
}

/// True when `name` uses a discouraged test-naming pattern.
///
/// Flags the bare `test` name, the redundant `test_*` / `case_*` prefixes, and
/// `test` immediately followed by digits (`test1`, `test2`). Behavioral names
/// like `should_pass_when_valid` pass.
fn is_bad_test_name(name: &str) -> bool {
    name == "test"
        || name.starts_with("test_")
        || name.starts_with("case_")
        || is_test_plus_digits(name)
}

/// True for names like `test1`, `test2` (`test` immediately followed by digits).
fn is_test_plus_digits(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("test") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::tests::parse_one;

    // ── TEST001: test_naming ──

    #[test]
    fn test_naming_test_prefix() {
        let item = parse_one("#[test]\nfn test_foo() {}");
        let diags = test_naming(&item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_TEST_NAMING);
    }

    #[test]
    fn test_naming_test_digits() {
        let item = parse_one("#[test]\nfn test1() {}");
        assert_eq!(test_naming(&item).len(), 1);
    }

    #[test]
    fn test_naming_case_prefix() {
        let item = parse_one("#[test]\nfn case_1() {}");
        assert_eq!(test_naming(&item).len(), 1);
    }

    #[test]
    fn test_naming_behavioral() {
        let item = parse_one("#[test]\nfn should_pass_when_valid() {}");
        assert!(test_naming(&item).is_empty());
    }

    #[test]
    fn test_naming_non_test() {
        let item = parse_one("fn helper() {}");
        assert!(test_naming(&item).is_empty());
    }
}
