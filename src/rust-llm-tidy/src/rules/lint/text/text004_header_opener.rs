//! TEXT004: header opener shape over the plaintext analysis.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_HEADER_OPENER;
use crate::text::measurement::{Document, Paragraph, ParagraphKind, StrippedLine};

/// Plain openers with more measured chars than this fire TEXT004.
///
/// Tighter than TEXT001's paragraph limit so the lead stays scannable even
/// when punctuation keeps the sentence count low. Bullet openers are exempt:
/// a list opener should not hold only its first bullet to a tighter budget.
const OPENER_CHAR_LIMIT: usize = 160;
/// Openers with more sentences than this fire TEXT004.
const OPENER_SENTENCE_LIMIT: usize = 2;

/// TEXT004 diagnostics for `doc`: one Warning per opener paragraph that
/// exceeds the sentence limit, or, when plain, the char limit.
///
/// Openers:
///
/// - The first measured paragraph of each doc region (the file, module, or
///   item opener, such as a method doc's leading paragraph).
/// - The first paragraph after each heading line: a stripped line outside
///   code blocks whose trimmed text starts with `#`. Adjacent headings
///   share one opener; a heading with no following paragraph checks
///   nothing.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let headings = heading_lines(&doc.lines);
    let mut diags = Vec::new();
    for (index, para) in doc.paragraphs.iter().enumerate() {
        if !is_opener(doc, &headings, index, para) {
            continue;
        }

        let sentences = sentence_count(&para.text);
        let over_sentences = sentences > OPENER_SENTENCE_LIMIT;
        let over_chars = para.kind == ParagraphKind::Plain && para.size > OPENER_CHAR_LIMIT;
        if over_sentences || over_chars {
            let summary = if over_sentences {
                format!(
                    "opener paragraph has {sentences} sentences; maximum is \
                     {OPENER_SENTENCE_LIMIT}."
                )
            } else {
                format!(
                    "opener paragraph is {} chars long; maximum is \
                     {OPENER_CHAR_LIMIT}.",
                    para.size
                )
            };
            diags.push(opener_diagnostic(para, &summary));
        }
    }
    diags
}

/// Line numbers of heading lines, in source order.
fn heading_lines(lines: &[StrippedLine]) -> Vec<usize> {
    lines
        .iter()
        .filter(|line| !line.in_code_block && line.text.trim_start().starts_with('#'))
        .map(|line| line.number)
        .collect()
}

/// True when `para` at `index` is a region opener (the first measured
/// paragraph of a doc region) or the first paragraph after a heading line.
fn is_opener(doc: &Document, headings: &[usize], index: usize, para: &Paragraph) -> bool {
    if para.opens_region {
        return true;
    }
    let previous_end = doc.paragraphs[index - 1]
        .line_starts
        .last()
        .map_or(para.first_line, |(line, _)| *line);
    headings
        .iter()
        .any(|heading| previous_end < *heading && *heading < para.first_line)
}

/// TEXT004 Warning for one opener paragraph, reported at its first line.
fn opener_diagnostic(para: &Paragraph, summary: &str) -> Diagnostic {
    let bullets = [
        "Keep the opener brief so readers can find the main point quickly.".to_string(),
        "Lead with the main point, ideally in one short sentence.".to_string(),
        format!("Keep a plain opener to {OPENER_CHAR_LIMIT} measured chars or fewer."),
        "Move supporting details below the opener without losing necessary \
          information."
            .to_string(),
        "Use bullets for distinct facts, one fact per bullet.".to_string(),
        "Keep a connected explanation in a separate short paragraph.".to_string(),
    ];
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_HEADER_OPENER,
        message: bulleted(summary, &bullets),
        line: para.first_line,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// Sentences in `text`: boundary count plus one.
///
/// A boundary is `.`, `!`, or `?` followed by whitespace whose next
/// non-whitespace char is not a lowercase letter (Unicode-aware).
///
/// A terminator at the end of `text` closes the final sentence
/// without adding a boundary.
///
/// The count is conservative: it can under-count (miss violations) but
/// never fabricate them, so `e.g.` before a lowercase word, decimals,
/// and URLs stay one sentence.
fn sentence_count(text: &str) -> usize {
    let mut count = 1;
    let chars: Vec<char> = text.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if !matches!(ch, '.' | '!' | '?') {
            continue;
        }
        // A boundary needs whitespace after the terminator: decimals and
        // URLs never split because no whitespace follows their periods.
        if !chars.get(i + 1).is_some_and(|next| next.is_whitespace()) {
            continue;
        }
        let Some(&next) = chars[i + 1..].iter().find(|c| !c.is_whitespace()) else {
            // Terminator at the end of the text: no new sentence follows.
            continue;
        };
        if !next.is_lowercase() {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use indoc::formatdoc;

    // ── Sentence count ──

    // A three-sentence heading opener -> one Warning at the paragraph's
    // first line.
    #[test]
    fn text_checks_warn_on_three_sentence_opener() {
        let source = formatdoc! {"
            # Title

            One. Two. Three.
        "};
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 3);
    }

    // A one-sentence opener stays silent: bullets no longer affect
    // TEXT004 (the narrative condition was removed).
    #[test]
    fn text_checks_silent_on_one_sentence_opener() {
        let source = "# Title\n\nOne sentence.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // A two-sentence opener stays at the limit: TEXT004 fires on three
    // or more sentences only.
    #[test]
    fn text_checks_silent_on_two_sentence_opener() {
        let source = "# Title\n\nOne. Two.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // `e.g.` before a lowercase word is not a sentence boundary.
    #[test]
    fn text_checks_treat_abbreviation_before_lowercase_as_one_sentence() {
        let source = "Use it, e.g. like this.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // Decimals and URLs never split: no whitespace follows their periods.
    #[test]
    fn text_checks_treat_decimals_and_urls_as_one_sentence() {
        let source = "See https://example.com/x at 3.5 miles.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // A boundary followed by an uppercase word splits past the limit:
    // three sentences.
    #[test]
    fn text_checks_split_at_terminator_before_uppercase() {
        let source = "One. Two. Three.";
        let diags = run_text_checks(source, "md");
        assert_eq!(codes(&diags, CODE_HEADER_OPENER).len(), 1);
    }

    // ── Heading classification ──

    // A heading line inside a code block is not a heading: the paragraph
    // after it is not an opener.
    #[test]
    fn text_checks_ignore_headings_inside_code_blocks() {
        let source = formatdoc! {"
            Paragraph one.

            ```
            # fenced heading
            ```

            Not an opener.
        "};
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // A doc heading like `# Errors` opens the paragraph that follows it.
    #[test]
    fn text_checks_check_paragraph_after_doc_heading() {
        let source = formatdoc! {"
            /// Module opener.
            ///
            /// # Errors
            ///
            /// Fails. The input is malformed. It was rejected.
        "};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 5);
    }

    // Adjacent headings yield one opener, checked once.
    #[test]
    fn text_checks_check_adjacent_headings_opener_once() {
        let source = "# A\n\n# B\n\nOne. Two. Three.\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 5);
    }

    // A heading with no following paragraph checks nothing.
    #[test]
    fn text_checks_silent_on_trailing_heading_without_paragraph() {
        let source = "Opener sentence.\n\n# Trailing\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // ── The document's first measured paragraph ──

    // A multi-sentence markdown file opener fires without any heading.
    #[test]
    fn text_checks_warn_on_multi_sentence_markdown_file_opener() {
        let source = "One sentence. Another sentence. A third sentence.\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    // A multi-sentence Rust `//!` module opener fires through the same
    // pipeline.
    #[test]
    fn text_checks_warn_on_multi_sentence_rust_module_opener() {
        let source = formatdoc! {"
            //! Module opener. It has three sentences. This is the third.
            //!
            //! # Section
            //!
            //! One sentence.
        "};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    // ── Region openers (item docs) ──

    // A later doc region's first paragraph is an opener: a
    // three-sentence method doc fires at its own line.
    #[test]
    fn text_checks_warn_on_three_sentence_method_opener() {
        let source = formatdoc! {"
            /// Module opener.

            /// Does a thing. It also does another. And a third.
            fn first() {{}}

            /// Later method opener.
            fn second() {{}}
        "};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    // A one-sentence later-region opener stays silent.
    #[test]
    fn text_checks_silent_on_one_sentence_method_opener() {
        let source = formatdoc! {"
            /// Module opener.

            /// Does a thing.
            fn later() {{}}
        "};
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // ── Char budget ──

    #[test]
    fn text_checks_should_ignore_borders_when_surrounding_short_opener() {
        for (name, left, marker, right) in [
            ("dashes", "", "-", ""),
            ("equals", "", "=", ""),
            ("underscores", "", "_", ""),
            ("asterisks", "", "*", ""),
            ("slashes", "", "/", ""),
            ("backslashes", "", "\\", ""),
            ("dots", "", ".", ""),
            ("colons", "", ":", ""),
            ("spaced dashes", "", "- ", ""),
            ("mixed ASCII", "+", "-=", "+"),
            ("light box", "┌", "─", "┐"),
            ("heavy box", "┏", "━", "┓"),
            ("double box", "╔", "═", "╗"),
            ("blocks", "", "█", ""),
        ] {
            let border = format!("{left}{}{right}", marker.repeat(OPENER_CHAR_LIMIT / 2));
            let source = format!("// {border}\n// Shared path resolution\n// {border}\n");

            let diags = run_text_checks(&source, "rs");

            assert!(
                codes(&diags, CODE_HEADER_OPENER).is_empty(),
                "{name}: {diags:?}"
            );
        }
    }

    #[test]
    fn text_checks_should_check_prose_when_surrounded_by_borders() {
        for prose in [
            "x".repeat(OPENER_CHAR_LIMIT + 1),
            "Sentence. ".repeat(OPENER_SENTENCE_LIMIT + 1),
        ] {
            let source = format!("// -----\n// {prose}\n// -----\n");

            let diags = run_text_checks(&source, "rs");

            let found = codes(&diags, CODE_HEADER_OPENER);
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].line, 2);
        }
    }

    // A one-sentence plain opener over the char limit warns with the char
    // cause: sentence count alone would let a dense lead pass.
    #[test]
    fn text_checks_warn_on_oversized_one_sentence_opener() {
        let source = formatdoc! {"
            # Title

            Loads the configured data from disk and parses it into the \
            schema while resolving every relative path against the \
            configured base directory for each caller in this process.
        "};
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
        assert!(found[0].message.starts_with("opener paragraph is 1"),);
        assert!(found[0].message.contains("chars long; maximum is 160.\n"));
    }

    // A bullet opener is exempt from the char budget: only the sentence
    // limit applies, so a list opener's first bullet is not singled out.
    #[test]
    fn text_checks_skip_char_budget_on_bullet_opener() {
        let source = formatdoc! {"
            # Title

            - Loads the configured data from disk and parses it into the \
            schema while resolving relative paths against the configured \
            base directory for every caller
        "};
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // A non-opener paragraph over the char limit stays silent: the budget
    // shapes headers, not body prose (TEXT001 owns that).
    #[test]
    fn text_checks_skip_char_budget_on_non_opener_paragraph() {
        let source = formatdoc! {"
            # Title

            Lead sentence.

            Loads the configured data from disk and parses it into the \
            schema while resolving relative paths against the configured \
            base directory for every caller.
        "};
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // A two-sentence opener over the char limit reports the char cause
    // once, not both causes.
    #[test]
    fn text_checks_report_char_cause_once_on_two_sentence_opener() {
        let source = formatdoc! {"
            # Title

            Loads the configured data from disk and parses it. It also \
            resolves every relative path against the configured base \
            directory for each caller in this whole process.
        "};
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("chars long; maximum is 160."));
        assert!(!found[0].message.contains("sentences; maximum"));
    }

    // ── Diagnostic shape and message ──

    // The summary reports the sentence count against the limit, with the
    // guidance bullets.
    #[test]
    fn text_checks_message_states_sentence_cause_and_guidance() {
        let source = "# T\n\nOne. Two. Three.\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        let msg = &found[0].message;
        assert!(msg.starts_with("opener paragraph has 3 sentences; maximum is 2.\n"));
        assert!(msg.contains("main point"));
        assert!(msg.contains("one fact per bullet"));
    }

    // The sentence count itself is unit-pinned on the helper.
    #[test]
    fn sentence_count_pins_conservative_boundaries() {
        assert_eq!(sentence_count("One."), 1);
        assert_eq!(sentence_count("One. Two."), 2);
        assert_eq!(sentence_count("e.g. like this."), 1);
        // `ü` is lowercase Unicode but not ASCII: the abbreviation must
        // still not split here.
        assert_eq!(sentence_count("e.g. über configuration. It works."), 2);
        assert_eq!(sentence_count("See 3.5 miles."), 1);
        assert_eq!(sentence_count("See https://example.com/x."), 1);
        assert_eq!(sentence_count("One! Two? Three."), 3);
    }
}
