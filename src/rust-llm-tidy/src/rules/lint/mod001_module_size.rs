//! Warn when a source file exceeds the MOD001 line budget.
//!
//! Non-Rust files count every physical line, including blanks, comments,
//! and tests. A recognized module header leaves the count when exclusion
//! is enabled ([`crate::text::comments::header_lines`]).
//!
//! The Rust rule supplies its test-aware count to [`diagnostic`].
//!
//! Diagnostics encourage cohesive splits rather than mechanical line
//! reduction. [`diagnostic`] states why size matters, then suggests split
//! patterns; the Rust rule appends its own advice and counting notes.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MODULE_SIZE;
use crate::text::comments;

/// Check a whole file against the resolved `module_size.max_lines` budget.
///
/// Exactly `max_lines` stays silent. A final line without a newline counts;
/// a trailing newline does not create an extra line.
///
/// With `exclude_module_headers`, the file's leading module-header lines
/// leave the count first. The reported line is then the physical line
/// holding the first counted line past the budget. Reports at
/// `max_lines + 1` otherwise.
///
/// # Arguments
///
/// - `source`: the file's raw text.
/// - `ext`: the file extension without the leading dot; selects the
///   header syntax.
/// - `max_lines`: the resolved `module_size.max_lines` budget.
/// - `exclude_module_headers`: the resolved `module_size.exclude_module_headers`.
pub(crate) fn check(
    source: &str,
    ext: &str,
    max_lines: usize,
    exclude_module_headers: bool,
) -> Option<Diagnostic> {
    let header = if exclude_module_headers {
        comments::header_lines(source, ext)
    } else {
        0
    };
    let lines = source.lines().count() - header;
    let exclusions = if header > 0 {
        " outside module headers"
    } else {
        ""
    };

    // The header is a contiguous prefix, so the `max_lines + 1`-th
    // counted line is the `max_lines + header + 1`-th physical line.
    (lines > max_lines).then(|| diagnostic(lines, max_lines + header + 1, max_lines, exclusions))
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
            Why:
            - Large files make readers search farther and keep more context in mind.
            - Focused modules help readers find responsibilities without scanning unrelated code.
            - Large files cost LLMs more input tokens when read in full and leave less
              context for other relevant code.
            Suggestions:
            - Consider keeping entry points and orchestration near the top level, with
              implementation details in focused child modules.
            - Group code by responsibility, such as parsing or validation. Domain names
              usually explain more than catch-all names like `utils`.
            - Let orchestration read as calls to clear operations, such as `parse_imports`
              or `validate_config`.
            - Free functions suit stateless work. Methods suit behavior that manages a
              type's state.
            - Keep closely related code together. A split need not add new types,
              forwarding wrappers, or a wider public API.
            - Preserve behavior and performance across the split. Avoid needless
              allocations, clones, or repeated work just to cross module boundaries.
            - Update overview docs to explain responsibilities and point readers to the
              entry points. Keep useful documentation; a split should not remove it."},
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
        let finding = check(source, "js", max_lines, false);

        assert_eq!(finding.as_ref().map(|d| d.line), expected_line);
        if let Some(finding) = finding {
            assert_eq!(finding.code, CODE_MODULE_SIZE);
            assert_eq!(finding.severity, Severity::Warning);
            assert!(finding.message.starts_with("file has 3 lines,\n"));
            assert!(finding.message.contains(
                "- Preserve behavior and performance across the split. Avoid needless\n  \
                 allocations, clones, or repeated work just to cross module boundaries."
            ));
            assert_eq!(finding.item_kind, "file");
            assert!(finding.item_name.is_none());
        }
    }

    /// The module header leaves the count; the crossing line keeps its
    /// physical position. Disabling the exclusion counts it again.
    #[rstest]
    #[case::docstring("py", "# header\n\"\"\"Doc.\n\"\"\"\nx = 1\ny = 2\n", 3, Some(5), true)]
    #[case::line_comments("js", "// note\n// note\nx = 1\ny = 2\n", 2, Some(4), true)]
    #[case::block_comments("cs", "/* Note. */\nclass A {{}}\nclass B {{}}\n", 1, Some(3), true)]
    #[case::no_header("py", "x = 1\ny = 2\n", 0, Some(2), true)]
    #[case::exclusion_disabled("js", "// note\nx = 1\n", 0, Some(2), false)]
    fn check_should_exclude_module_headers_from_the_budget(
        #[case] ext: &str,
        #[case] source: &str,
        #[case] header: usize,
        #[case] expected_line: Option<usize>,
        #[case] exclude_module_headers: bool,
    ) {
        let finding = check(source, ext, 1, exclude_module_headers);

        assert_eq!(finding.as_ref().map(|d| d.line), expected_line);
        if let (true, true, Some(finding)) = (exclude_module_headers, header > 0, finding.as_ref())
        {
            let counted = source.lines().count() - header;
            assert!(
                finding.message.starts_with(&format!(
                    "file has {counted} lines outside module headers,\n"
                )),
                "{}",
                finding.message
            );
        }
    }
}
