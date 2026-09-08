//! Warn when a source file exceeds the MOD001 line budget.
//!
//! Non-Rust files count every physical line, including blanks, comments, and
//! tests. The Rust rule supplies its test-aware count to [`diagnostic`].
//! Diagnostics encourage cohesive splits rather than mechanical line reduction.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MODULE_SIZE;

/// Check a whole file against the resolved `module_size.max_lines` budget.
///
/// Exactly `max_lines` stays silent. A final line without a newline counts;
/// a trailing newline does not create an extra line. Reports at `max_lines + 1`.
pub(crate) fn check(source: &str, max_lines: usize) -> Option<Diagnostic> {
    let lines = source.lines().count();

    (lines > max_lines).then(|| diagnostic(lines, max_lines + 1, max_lines, ""))
}

/// Build the shared warning with a language-specific description of excluded lines.
pub(super) fn diagnostic(
    lines: usize,
    crossing_line: usize,
    max_lines: usize,
    exclusions: &str,
) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_MODULE_SIZE,
        message: indoc::formatdoc! {"
            file has {lines} lines{exclusions},
            over the {max_lines}-line budget (module_size.max_lines).
            - Large files make readers search farther and keep more context in mind.
            - Split distinct responsibilities into focused, domain-named modules.
              Keep closely related code together.
            - Keep a clear starting point for callers and readers. Consider keeping
              main entry points and high-level orchestration together, with
              implementation details in focused modules or types.
            - Preserve the intended interface without widening visibility or
              adding forwarding wrappers just to centralize entry points.
            - Update module or type overview docs, where supported, to explain
              responsibilities and direct readers to relevant entry points.
            - Do not split mechanically or remove useful documentation to meet
              the line budget."},
        line: crossing_line,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Physical-line boundaries do not depend on newline style or line content.
    #[rstest]
    #[case::empty("", 1, None)]
    #[case::below_budget("code\n", 2, None)]
    #[case::at_budget("code\n\n", 2, None)]
    #[case::over_budget("code\n\n// comment\n", 2, Some(3))]
    #[case::unterminated_last_line("code\n\n// comment", 2, Some(3))]
    #[case::crlf("code\r\n\r\n// comment\r\n", 2, Some(3))]
    #[case::maximum_budget("code\n", usize::MAX, None)]
    fn check_should_report_physical_lines_over_budget(
        #[case] source: &str,
        #[case] max_lines: usize,
        #[case] expected_line: Option<usize>,
    ) {
        let finding = check(source, max_lines);

        assert_eq!(finding.as_ref().map(|d| d.line), expected_line);
        if let Some(finding) = finding {
            assert_eq!(finding.code, CODE_MODULE_SIZE);
            assert_eq!(finding.severity, Severity::Warning);
            assert!(finding.message.starts_with("file has 3 lines,\n"));
            assert_eq!(finding.item_kind, "file");
            assert!(finding.item_name.is_none());
        }
    }
}
