//! DOC008: line length limit over the plaintext analysis.

use super::{Document, StrippedLine, bulleted};
use crate::check::CODE_LINE_LENGTH;
use crate::diagnostic::{Diagnostic, Severity};

/// Maximum line length before DOC008 fires.
const LINE_LIMIT: usize = 80;

/// DOC008 diagnostics for `doc`: one Warning per line over the limit, in
/// source order.
///
/// Measurement per line kind:
///
/// - Code-block lines, table rows, and link reference definitions are
///   skipped.
/// - Every other line counts in full: code spans, URLs, and link targets
///   included.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in &doc.lines {
        if line.in_code_block {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.starts_with('|') || super::is_link_reference_definition(trimmed) {
            continue;
        }
        let len = trimmed.chars().count();
        if len > LINE_LIMIT {
            diags.push(line_length_diagnostic(line, len));
        }
    }
    diags
}

/// DOC008 Warning for one over-limit line; `len` is the full line length.
fn line_length_diagnostic(line: &StrippedLine, len: usize) -> Diagnostic {
    let bullets = [
        format!(
            "Lines over {LINE_LIMIT} chars strain short attention spans \
             and need wide monitors."
        ),
        "Split it at the nearest idea change with a blank line.".to_string(),
        "Code spans, URLs, and link targets count.".to_string(),
        "Code blocks, table rows, and link definitions are exempt.".to_string(),
    ];
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_LINE_LENGTH,
        message: bulleted(&format!("line is {len} chars long."), &bullets),
        line: line.number,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::plaintext::run_text_checks;
    use crate::check::plaintext::tests::codes;
    use indoc::formatdoc;

    // ── DOC008: line length ──

    // Over-limit stripped line -> DOC008 Warning with a measurement summary
    // plus rationale and fix bullets. Indent and comment marker are not
    // measured.
    #[test]
    fn text_checks_warn_on_long_stripped_line() {
        let text = "x".repeat(81);
        let source = format!("\t/// {text}\n");
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.starts_with("line is 81 chars long."));
        assert!(msg.contains("strain short attention spans"));
        assert!(msg.contains("blank line"));
        assert!(msg.contains("Code spans, URLs, and link targets count"));
        assert!(msg.contains("Code blocks, table rows, and link definitions are exempt"));
    }

    // A line of exactly 80 chars passes.
    #[test]
    fn text_checks_silent_on_line_at_limit() {
        let source = format!("{}\n", "x".repeat(80));
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // Code-block lines are exempt: fenced and indented blocks never warn,
    // including an over-long fence delimiter with an info string.
    #[test]
    fn text_checks_silent_on_code_block_lines() {
        let fenced = "y".repeat(90);
        let fence_line = format!("```{}", "i".repeat(80));
        let md = formatdoc! {"
            {fence_line}
            {fenced}
            ```

                {fenced}
        "};
        assert!(codes(&run_text_checks(&md, "md"), CODE_LINE_LENGTH).is_empty());
        let indented_rs = format!("///     {}\n", "y".repeat(90));
        assert!(codes(&run_text_checks(&indented_rs, "rs"), CODE_LINE_LENGTH).is_empty());
    }

    // The same span-bearing text stays silent inside a code block but warns
    // as prose, where its code span counts.
    #[test]
    fn text_checks_exempt_code_block_but_count_prose_spans() {
        let inner = format!("say `{}` out loud", "b".repeat(80));
        let source = formatdoc! {"
            ```
            {inner}
            ```

            {inner}
        "};
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 5);
    }

    // Table rows are never measured, in Markdown and doc comments alike.
    #[test]
    fn text_checks_silent_on_table_rows() {
        let row = format!("| {} |", "cell ".repeat(20));
        let md = formatdoc! {"
            | a | b |
            | --- | --- |
            {row}
        "};
        assert!(codes(&run_text_checks(&md, "md"), CODE_LINE_LENGTH).is_empty());
        let rs = format!("/// {row}\n");
        assert!(codes(&run_text_checks(&rs, "rs"), CODE_LINE_LENGTH).is_empty());
    }

    // Link reference definitions are skipped whole, long labels included.
    #[test]
    fn text_checks_silent_on_link_reference_definitions() {
        let source = format!("[{}]: ./a/very/long/relative/target.md\n", "l".repeat(90));
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // Code spans and URLs count: a line over the limit through its span
    // and URL warns with the full length.
    #[test]
    fn text_checks_count_code_spans_and_urls() {
        let source = format!(
            "see `{}` at https://example.com/paths/now\n",
            "c".repeat(50)
        );
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert!(found[0].message.starts_with("line is 89 chars long."));
    }

    // The warning reports the full line length, spans included.
    #[test]
    fn text_checks_reports_full_length_with_spans() {
        let source = format!("{} `{}`\n", "a".repeat(82), "b".repeat(20));
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.starts_with("line is 105 chars long."));
    }

    // Badge lines count in full: image target and reference tail included.
    #[test]
    fn text_checks_count_badge_line_targets() {
        let source = "[![Crates.io](https://img.shields.io/badge/rust_llm_tidy-v0.1.0-orange.svg)][Crates.io CLI]\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.starts_with("line is 91 chars long."));
    }
}
