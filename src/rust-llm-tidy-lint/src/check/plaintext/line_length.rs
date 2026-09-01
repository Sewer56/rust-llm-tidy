//! DOC008: stripped line length limit over the plaintext analysis.

use super::{Document, StrippedLine, bulleted};
use crate::check::CODE_LINE_LENGTH;
use crate::diagnostic::{Diagnostic, Severity};

/// Maximum stripped line length before DOC008 fires.
const LINE_LIMIT: usize = 80;

/// DOC008 diagnostics for `doc`: one Warning per stripped line over the limit,
/// in source order.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in &doc.lines {
        let len = line.text.chars().count();
        if len > LINE_LIMIT {
            diags.push(line_length_diagnostic(line, len));
        }
    }
    diags
}

/// DOC008 Warning for one over-limit stripped line.
fn line_length_diagnostic(line: &StrippedLine, len: usize) -> Diagnostic {
    let bullets = [
        format!(
            "Lines over {LINE_LIMIT} chars strain short attention spans \
             and need wide monitors."
        ),
        "Split it at the nearest idea change with a blank line.".to_string(),
        "Code-block lines count too.".to_string(),
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
        assert!(msg.contains("Code-block lines count too"));
    }

    // A line of exactly 80 chars passes.
    #[test]
    fn text_checks_silent_on_line_at_limit() {
        let source = format!("{}\n", "x".repeat(80));
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // No content exemptions: fenced code-block lines also warn.
    #[test]
    fn text_checks_warn_on_long_lines_inside_fenced_code() {
        let inner = "y".repeat(90);
        let source = formatdoc! {"
            ```
            {inner}
            ```
        "};
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
    }
}
