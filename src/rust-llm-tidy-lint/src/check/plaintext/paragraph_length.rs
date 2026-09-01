//! DOC007: paragraph and bullet size limits over the plaintext analysis.

use super::{Document, Paragraph, ParagraphKind, bulleted};
use crate::check::CODE_PARAGRAPH_SIZE;
use crate::diagnostic::{Diagnostic, Severity};

/// Recommended maximum bullet length, stated in the shortening guidance.
const BULLET_RECOMMENDED: usize = 160;
/// Maximum measured paragraph size before DOC007 fires.
const PARAGRAPH_LIMIT: usize = 240;

/// DOC007 diagnostics for `doc`: one per paragraph whose measured size exceeds
/// the limit, in source order.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for para in &doc.paragraphs {
        if para.size <= PARAGRAPH_LIMIT {
            continue;
        }
        diags.push(match para.kind {
            ParagraphKind::Plain => paragraph_diagnostic(para),
            ParagraphKind::Bullet => bullet_diagnostic(para),
        });
    }
    diags
}

/// DOC007 Warning for an over-limit bullet, with shortening guidance.
fn bullet_diagnostic(para: &Paragraph) -> Diagnostic {
    let bullets = [
        format!("Bullets over {PARAGRAPH_LIMIT} chars outlast a short attention span."),
        format!(
            "Shorten it to one checkable action of at most \
             {BULLET_RECOMMENDED} chars."
        ),
        "Split it into separate bullets.".to_string(),
    ];
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_PARAGRAPH_SIZE,
        message: bulleted(&format!("bullet is {} chars long.", para.size), &bullets),
        line: para.first_line,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// DOC007 Error for an over-limit plain paragraph, reported at its first line.
fn paragraph_diagnostic(para: &Paragraph) -> Diagnostic {
    let bullets = [
        format!("Paragraphs over {PARAGRAPH_LIMIT} chars outlast a short attention span."),
        "Split it at the nearest idea change with a blank line.".to_string(),
        "Convert list-like paragraphs into bullets.".to_string(),
        format!(
            "Keep each bullet to one checkable action of at most \
             {BULLET_RECOMMENDED} chars."
        ),
        "Move remarks into their own sections.".to_string(),
        "The check skips code blocks, tables, headings, signature lines, and link definitions."
            .to_string(),
    ];
    Diagnostic {
        severity: Severity::Error,
        code: CODE_PARAGRAPH_SIZE,
        message: bulleted(&format!("paragraph is {} chars long.", para.size), &bullets),
        line: para.first_line,
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

    // Builds a `///` comment paragraph of `words` filler words.
    fn paragraph_source(words: usize) -> String {
        (0..words)
            .map(|i| format!("/// w{i}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    // ── DOC007: paragraph and bullet budgets ──

    // Over-limit plain paragraph -> DOC007 Error at the first line, with a
    // measurement summary plus rationale and fix bullets.
    #[test]
    fn text_checks_error_on_oversized_plain_paragraph() {
        let source = paragraph_source(80);
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(
            msg.starts_with("paragraph is "),
            "message must open with the measurement"
        );
        assert!(msg.contains("chars long.\n"));
        assert!(msg.contains("outlast a short attention span"));
        assert!(msg.contains("blank line"));
        assert!(msg.contains("bullets"));
        assert!(msg.contains("160"));
        assert!(msg.contains(
            "The check skips code blocks, tables, headings, signature lines, and link definitions."
        ));
    }

    // Paragraph at or under the limit is silent.
    #[test]
    fn text_checks_silent_on_paragraph_within_limit() {
        let source = paragraph_source(30);
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // Paragraph size counts chars, not bytes: 240 multibyte chars are over
    // 240 bytes yet must stay at the limit and stay silent.
    #[test]
    fn text_checks_measure_multibyte_paragraph_in_chars() {
        let source = format!("/// {}\n", "é".repeat(240));
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // A paragraph measuring exactly 240 chars is at the limit, not over it.
    #[test]
    fn text_checks_silent_on_paragraph_at_exact_limit() {
        let source = format!("/// {}\n", "x".repeat(240));
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // The Error is reported at the paragraph's first line, not where the
    // budget overflowed.
    #[test]
    fn text_checks_report_paragraph_at_its_first_line() {
        let source = formatdoc! {"
            /// short intro

            {}
        ", "/// ".to_string() + &"y".repeat(300)};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    // Over-limit bullet -> DOC007 Warning only, with shortening guidance and
    // the 160-char recommendation.
    #[test]
    fn text_checks_warn_on_oversized_bullet() {
        let bullet = "- ".to_string() + &"word ".repeat(60);
        let source = format!("{bullet}\n");
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.starts_with("bullet is "));
        assert!(msg.contains("outlast a short attention span"));
        assert!(msg.contains("Shorten"));
        assert!(msg.contains("160"));
        assert!(msg.contains("separate bullets"));
    }

    // Bullet within the limit is silent.
    #[test]
    fn text_checks_silent_on_bullet_within_limit() {
        let source = format!("- {}\n", "word ".repeat(10));
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // Bullet content never inflates the enclosing paragraph's budget.
    #[test]
    fn text_checks_exclude_bullets_from_paragraph_budget() {
        let prose = "sentence ".repeat(20);
        let bullet = "- ".to_string() + &"word ".repeat(20);
        // Prose alone stays under 240; adding the bullet words would cross it.
        let source = format!("{prose}\n{bullet}\n");
        assert!(prose.trim().len() < 240);
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // Code-span chars count toward the paragraph budget: a span whose chars
    // push the total over the limit fires DOC007.
    #[test]
    fn text_checks_count_code_span_chars_in_paragraph_budget() {
        let prose = "x".repeat(200);
        let span = format!("`{}`", "y".repeat(60));
        let source = format!("/// {prose} {span}\n");
        // Measured 263 chars: the span counts in full.
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 1);
    }

    // Span and URL lines stay paragraph members: the lines join into one
    // paragraph whose full text crosses the limit, firing one Error.
    #[test]
    fn text_checks_error_on_paragraph_joined_across_segment_lines() {
        let source = formatdoc! {"
            /// Loads `config` from the given path.
            /// Docs live at https://example.com/config.
            /// Merge [defaults](crate::defaults) after loading.
            /// {}
        ", "z".repeat(200)};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 1);
    }

    // Code-span chars count toward the bullet budget: a bullet whose span
    // pushes it over the limit warns.
    #[test]
    fn text_checks_count_code_spans_in_bullet_budget() {
        let bullet = format!(
            "- {} `cargo test --all-features --workspace --all-targets`",
            "word ".repeat(40)
        );
        let source = format!("{bullet}\n");
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
    }
}
