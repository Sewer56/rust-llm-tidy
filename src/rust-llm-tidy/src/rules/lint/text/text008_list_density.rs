//! TEXT008: source-line budget for consecutive measured bullet paragraphs.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_TEXT008;
use crate::text::measurement::{Document, ParagraphKind};

/// Maximum total member lines before a bullet list warns.
const LIST_LINE_LIMIT: usize = 10;

/// Warn once per over-budget run of bullet paragraphs, at its first line.
///
/// Only a non-bullet paragraph ends a run. Blank lines do not contribute
/// to the budget; nested bullets contribute their own member lines.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut first_line = 0;
    let mut lines = 0;

    for para in &doc.paragraphs {
        if para.kind != ParagraphKind::Bullet {
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
    #[case::ordered("1. item\n".repeat(LIST_LINE_LIMIT + 1), 1)]
    #[case::fenced(format!("~~~text\n{}~~~\n", "- item\n".repeat(LIST_LINE_LIMIT + 1)), 0)]
    fn checks_should_report_over_budget_lists(#[case] source: String, #[case] expected: usize) {
        let diags = run_text_checks(&source, "md");

        assert_eq!(codes(&diags, CODE_TEXT008).len(), expected);
    }

    // Independent runs and diagnostic rendering.

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
