//! TEXT008: source-line budget for consecutive unordered bullet paragraphs.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_TEXT008;
use crate::text::measurement::{Document, ParagraphKind};

/// Maximum total member lines before a bullet list warns.
const LIST_LINE_LIMIT: usize = 10;

/// Warn once per over-budget run of bullet paragraphs, at its first line.
///
/// Prose, numbered lists, headings, and code blocks end a run.
/// Headings follow the measuring core's `#`-prefix convention. Blank lines
/// do not count; nested bullets contribute their own member lines.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut source_lines = doc.lines.iter().peekable();
    let mut diags = Vec::new();
    let mut first_line = 0;
    let mut lines = 0;

    for para in &doc.paragraphs {
        // Scan each source line once, including after an over-budget list.
        while let Some(line) = source_lines.next_if(|line| line.number < para.first_line) {
            if line.in_code_block || line.text.trim_start().starts_with('#') {
                lines = 0;
            }
        }

        // The measuring core recognizes digit-prefixed bullets as ordered lists.
        let numbered = source_lines.peek().is_some_and(|line| {
            line.number == para.first_line
                && line
                    .text
                    .trim_start()
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_digit)
        });
        if para.kind != ParagraphKind::Bullet || numbered {
            lines = 0;
            continue;
        }
        if lines > LIST_LINE_LIMIT {
            continue;
        }
        if lines == 0 {
            first_line = para.first_line;
        }

        lines += para.line_starts.len();
        if lines > LIST_LINE_LIMIT {
            diags.push(list_diagnostic(first_line));
        }
    }

    diags
}

/// Report an over-budget list with shortening and grouping guidance.
fn list_diagnostic(line: usize) -> Diagnostic {
    let bullets = [
        "Long bullet lists can be hard to scan, especially when items wrap across lines."
            .to_string(),
        "Tighten each bullet to a single line.".to_string(),
        "If the list needs more room, split the bullets into groups with subheadings or a table."
            .to_string(),
    ];

    Diagnostic {
        severity: Severity::Warning,
        code: CODE_TEXT008,
        message: bulleted(
            &format!("bullet list spans more than {LIST_LINE_LIMIT} lines."),
            &bullets,
        ),
        line,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use rstest::rstest;

    // Budget boundaries and mixed item lengths.

    #[rstest]
    #[case::six_wrapped("- item\n  tail\n".repeat(LIST_LINE_LIMIT / 2 + 1), 1)]
    #[case::four_three_line("- item\n  tail\n  end\n".repeat(LIST_LINE_LIMIT / 3 + 1), 1)]
    #[case::eleven_single("- item\n".repeat(LIST_LINE_LIMIT + 1), 1)]
    #[case::seven_single("- item\n".repeat(LIST_LINE_LIMIT - 3), 0)]
    #[case::eight_single("- item\n".repeat(LIST_LINE_LIMIT - 2), 0)]
    #[case::five_wrapped("- item\n  tail\n".repeat(LIST_LINE_LIMIT / 2), 0)]
    #[case::ten_single("- item\n".repeat(LIST_LINE_LIMIT), 0)]
    #[case::mixed(format!("{}- item\n  tail", "- item\n".repeat(LIST_LINE_LIMIT - 1)), 1)]
    #[case::empty(String::new(), 0)]
    #[case::short("Short prose.".to_string(), 0)]
    #[case::eof("- item\n".repeat(LIST_LINE_LIMIT + 1).trim_end().to_string(), 1)]
    #[case::blank_gaps("- item\n\n".repeat(LIST_LINE_LIMIT + 1), 1)]
    #[case::blank_lines_excluded("- item\n\n".repeat(LIST_LINE_LIMIT), 0)]
    #[case::nested(format!("- parent\n{}", "  - child\n".repeat(LIST_LINE_LIMIT)), 1)]
    #[case::ordered_dot("1. item\n".repeat(LIST_LINE_LIMIT + 1), 0)]
    #[case::ordered_parenthesis("1) item\n".repeat(LIST_LINE_LIMIT + 1), 0)]
    #[case::ordered_wrapped("12. item\n    tail\n".repeat(LIST_LINE_LIMIT), 0)]
    #[case::fenced(format!("~~~text\n{}~~~\n", "- item\n".repeat(LIST_LINE_LIMIT + 1)), 0)]
    fn checks_should_report_over_budget_lists(#[case] source: String, #[case] expected: usize) {
        let diags = run_text_checks(&source, "md");

        assert_eq!(codes(&diags, CODE_TEXT008).len(), expected);
    }

    // Independent runs and diagnostic rendering.

    #[rstest]
    #[case::heading_at_budget("# Section", LIST_LINE_LIMIT, 0)]
    #[case::subheading_at_budget("## Section", LIST_LINE_LIMIT, 0)]
    #[case::deep_heading("###### Section", LIST_LINE_LIMIT, 0)]
    #[case::indented_heading("  ## Section", LIST_LINE_LIMIT, 0)]
    #[case::adjacent_headings("# Section\n## Subsection", LIST_LINE_LIMIT, 0)]
    #[case::both_dense("## Section", LIST_LINE_LIMIT + 1, 2)]
    #[case::blank_gap("", LIST_LINE_LIMIT, 1)]
    #[case::ordered_list("1. Step\n2. Next step", LIST_LINE_LIMIT, 0)]
    #[case::fenced_heading("```md\n# Example\n```", LIST_LINE_LIMIT, 0)]
    #[case::backtick_code("```rust\nlet n = 1;\n```", LIST_LINE_LIMIT, 0)]
    #[case::tilde_code("~~~text\nexample\n~~~", LIST_LINE_LIMIT, 0)]
    #[case::indented_code("    example", LIST_LINE_LIMIT, 0)]
    #[case::code_between_dense_lists("```text\nexample\n```", LIST_LINE_LIMIT + 1, 2)]
    #[case::table("| Example |", LIST_LINE_LIMIT, 1)]
    fn checks_should_apply_list_boundaries(
        #[case] separator: &str,
        #[case] list_lines: usize,
        #[case] expected: usize,
    ) {
        let list = "- item\n".repeat(list_lines);
        let source = format!("{list}\n{separator}\n\n{list}");

        let diags = run_text_checks(&source, "md");

        let found = codes(&diags, CODE_TEXT008);
        assert_eq!(found.len(), expected);
        if expected == 2 {
            assert_eq!(found[0].line, 1);
            assert_eq!(found[1].line, list_lines + separator.lines().count() + 3);
        }
    }

    #[rstest]
    #[case::both_short(LIST_LINE_LIMIT, LIST_LINE_LIMIT, vec![])]
    #[case::first_dense(LIST_LINE_LIMIT + 1, LIST_LINE_LIMIT, vec![1])]
    #[case::second_dense(LIST_LINE_LIMIT, LIST_LINE_LIMIT + 1, vec![LIST_LINE_LIMIT + 4])]
    #[case::both_dense(LIST_LINE_LIMIT + 1, LIST_LINE_LIMIT + 1, vec![1, LIST_LINE_LIMIT + 5])]
    fn checks_should_evaluate_lists_independently_when_prose_separates_them(
        #[case] first: usize,
        #[case] second: usize,
        #[case] expected_lines: Vec<usize>,
    ) {
        let source = format!(
            "{}\nSeparator.\n\n{}",
            "- item\n".repeat(first),
            "- item\n".repeat(second),
        );

        let diags = run_text_checks(&source, "md");

        let lines: Vec<_> = codes(&diags, CODE_TEXT008).iter().map(|d| d.line).collect();
        assert_eq!(lines, expected_lines);
    }

    #[test]
    fn checks_should_report_warning_at_list_start_with_remediation() {
        let source = format!("Intro.\n\n{}", "- item\n  tail\n".repeat(LIST_LINE_LIMIT));

        let diags = run_text_checks(&source, "md");

        let found = codes(&diags, CODE_TEXT008);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "TEXT008");
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 3);
        assert_eq!(found[0].item_kind, "file");
        assert_eq!(found[0].item_name, None);
        assert_eq!(found[0].title(), "dense bullet list");
        assert_eq!(
            found[0].message,
            format!(
                "bullet list spans more than {LIST_LINE_LIMIT} lines.\n  \
                 - Long bullet lists can be hard to scan, especially when items wrap across lines.\n  \
                 - Tighten each bullet to a single line.\n  \
                 - If the list needs more room, split the bullets into groups with subheadings or a table."
            )
        );
    }
}
