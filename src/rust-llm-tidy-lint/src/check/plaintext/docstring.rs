//! The docstring dialect: measuring [`DocRegion`]s whose lines carry
//! Python docstring content, the first-statement triple-quoted string of
//! a module, class, or function.
//!
//! The producer strips the quotes and the docstring's common
//! indentation. This dialect then exempts doctest examples and feeds the
//! remaining prose to the shared markdown classifier, so blank lines
//! split paragraphs.
//!
//! Fenced and indented example blocks are exempt through that
//! classifier.
//!
//! [`DocRegion`]: super::region::DocRegion

use super::region::DocRegion;
use super::{Document, PendingParagraph, StrippedLine, flush, measure_prose_line};

/// Measures one docstring region into `doc`'s lines and paragraphs.
///
/// Paragraph and fence state flow exactly as for the markdown dialect, so
/// prose never outlives the region: the measuring core flushes at every
/// region boundary.
pub(super) fn measure_region(
    region: DocRegion,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
    in_fence: &mut bool,
) {
    // A `>>>` line opens a doctest example that owns every following
    // line until the blank line ending the example: the source, `...`
    // continuations, and expected output are literal text, never prose.
    let mut in_doctest = false;
    for line in region.lines {
        let trimmed = line.text.trim();
        // A fence delimiter ends the example and flows through the
        // classifier, which closes the fence: swallowing it as doctest
        // output would leave the fence open and exempt the docstring's
        // remaining prose.
        if in_doctest && !trimmed.is_empty() && !is_fence_delimiter(trimmed) {
            doc.lines.push(StrippedLine {
                number: line.number,
                text: line.text,
                in_code_block: true,
            });
            continue;
        }
        in_doctest = false;
        // A `>>>` line inside an open fence is fenced example content,
        // not a doctest prompt, so it never opens an example there.
        if trimmed.starts_with(">>>") && !*in_fence {
            flush(pending, doc);
            doc.lines.push(StrippedLine {
                number: line.number,
                text: line.text,
                in_code_block: true,
            });
            in_doctest = true;
        } else {
            measure_prose_line(
                line.text,
                line.number,
                line.indented,
                doc,
                pending,
                in_fence,
            );
        }
    }
}

/// Whether the line is a fence delimiter, matching the shared
/// classifier's fence predicate (` ``` ` or `~~~` lead).
fn is_fence_delimiter(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

#[cfg(test)]
mod tests {
    use crate::check::CODE_LINE_LENGTH;
    use crate::check::CODE_PARAGRAPH_SIZE;
    use crate::check::plaintext::region::{Dialect, DocRegion, RegionLine};
    use crate::check::plaintext::run_region_checks;
    use crate::check::plaintext::tests::codes;
    use crate::diagnostic::Diagnostic;

    /// Builds one docstring region from `(line number, text)` pairs.
    fn docstring_region(lines: &[(usize, &str)]) -> DocRegion {
        DocRegion {
            dialect: Dialect::Docstring,
            lines: lines
                .iter()
                .map(|(number, text)| RegionLine {
                    number: *number,
                    text: (*text).to_string(),
                    indented: false,
                })
                .collect(),
        }
    }

    /// Builds one docstring region whose lines carry the producer's
    /// indented-code fact.
    fn docstring_region_with_indented(lines: &[(usize, &str, bool)]) -> DocRegion {
        DocRegion {
            dialect: Dialect::Docstring,
            lines: lines
                .iter()
                .map(|(number, text, indented)| RegionLine {
                    number: *number,
                    text: (*text).to_string(),
                    indented: *indented,
                })
                .collect(),
        }
    }

    /// Runs the text checks over one docstring region.
    fn measure(lines: &[(usize, &str)]) -> Vec<Diagnostic> {
        run_region_checks(vec![docstring_region(lines)])
    }

    // ── Prose measurement ──

    // Docstring lines join into one paragraph measured at the
    // paragraph's first line; a single over-80 line warns on its own.
    #[test]
    fn docstring_lines_join_into_paragraphs() {
        let filler = "word ".repeat(26);
        let line = filler.trim();
        let diags = measure(&[(2, "Summary."), (3, ""), (4, line), (5, line)]);

        let paragraphs = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].line, 4, "the first prose line");

        let long = "x".repeat(81);
        let diags = measure(&[(7, &long)]);
        let lines = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line, 7);
    }

    // A blank line splits docstring paragraphs: two over-half-budget
    // halves stay silent instead of joining past the limit.
    #[test]
    fn blank_lines_split_docstring_paragraphs() {
        let half = "h".repeat(130);
        let diags = measure(&[(1, &half), (2, ""), (3, &half)]);

        assert!(
            codes(&diags, CODE_PARAGRAPH_SIZE).is_empty(),
            "the halves must measure as separate paragraphs"
        );
    }

    // ── Doctest exemption ──

    // A doctest example - source, `...` continuation, and expected
    // output lines - measures as code: long lines stay quiet and no
    // prose pools across the example.
    #[test]
    fn doctest_examples_are_exempt() {
        let example = "c".repeat(90);
        let diags = measure(&[
            (1, "Runs the loader."),
            (2, ">>> value = load(key)"),
            (3, &format!(">>> process({example})")),
            (4, "... continue"),
            (5, &example),
            (6, ""),
            (7, "Returns the value."),
        ]);

        assert!(diags.is_empty(), "the doctest block stays fully quiet");
    }

    // Prose before and after a doctest never joins: two over-half-budget
    // halves around an example stay separate paragraphs.
    #[test]
    fn doctests_split_paragraphs() {
        let half = "h".repeat(130);
        let diags = measure(&[
            (1, &half),
            (2, ">>> load(key)"),
            (3, "42"),
            (4, ""),
            (5, &half),
        ]);

        assert!(
            codes(&diags, CODE_PARAGRAPH_SIZE).is_empty(),
            "prose must not join across the example"
        );
    }

    // A doctest example ends at its blank line: prose after the blank
    // measures again, and a new example after prose reopens the
    // exemption.
    #[test]
    fn doctests_end_at_blank_lines() {
        let long = "x".repeat(81);
        let diags = measure(&[
            (1, ">>> one"),
            (2, ""),
            (3, &long),
            (4, ""),
            (5, ">>> two"),
            (6, &long),
        ]);

        let lines = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(lines.len(), 1, "only the post-example prose warns");
        assert_eq!(lines[0].line, 3);
    }

    // ── Example blocks ──

    // Fenced blocks inside a docstring are exempt, delimiters included.
    #[test]
    fn fenced_examples_are_exempt() {
        let long = "c".repeat(90);
        let diags = measure(&[
            (1, "Summary."),
            (2, "```"),
            (3, &long),
            (4, "```"),
            (5, "More prose."),
        ]);

        assert!(diags.is_empty(), "the fenced example stays quiet");
    }

    // A `>>>` line inside a fenced example is fenced content, and the
    // closing delimiter closes the fence: prose after the block
    // measures, never silently exempt for the rest of the docstring.
    #[test]
    fn fenced_doctests_do_not_swallow_the_closing_fence() {
        for fence in ["```", "~~~"] {
            let long = "x".repeat(81);
            let diags = measure(&[
                (1, "Summary."),
                (2, fence),
                (3, ">>> run(x)"),
                (4, fence),
                (5, ""),
                (6, &long),
            ]);

            let lines = codes(&diags, CODE_LINE_LENGTH);
            assert_eq!(
                lines.len(),
                1,
                "fence `{fence}`: only the post-example prose warns"
            );
            assert_eq!(lines[0].line, 6);
        }
    }

    // A fenced example holding a doctest stays fully quiet, delimiters
    // and long doctest lines included.
    #[test]
    fn fenced_doctest_content_stays_quiet() {
        let example = "c".repeat(90);
        let diags = measure(&[
            (1, "Summary."),
            (2, "```"),
            (3, ">>> run(x)"),
            (4, &example),
            (5, "```"),
            (6, "More prose."),
        ]);

        assert!(diags.is_empty(), "the fenced doctest stays quiet");
    }

    // A line the producer marked indented is example code and exempt.
    #[test]
    fn indented_examples_are_exempt() {
        let long = "c".repeat(90);
        let region = docstring_region_with_indented(&[
            (1, "Summary.", false),
            (2, &long, true),
            (3, "More prose.", false),
        ]);
        let diags = run_region_checks(vec![region]);

        assert!(diags.is_empty(), "the indented example stays quiet");
    }
}
