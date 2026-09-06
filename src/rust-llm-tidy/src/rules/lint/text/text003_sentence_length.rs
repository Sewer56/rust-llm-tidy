//! TEXT003: sentence length limit over the plaintext analysis.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_SENTENCE_LENGTH;
use crate::text::measurement::{Document, Paragraph};

/// Characters that may close a sentence directly after its terminal
/// punctuation without opening a new word.
const CLOSERS: [char; 3] = ['"', '\'', ')'];
/// Maximum sentence length in words before TEXT003 fires.
const SENTENCE_LIMIT: usize = 25;
/// Recommended sentence length in words, stated in the guidance.
const SENTENCE_RECOMMENDED: usize = 14;

/// TEXT003 diagnostics for `doc`: one Warning per sentence over the word
/// limit, in source order.
///
/// Sentence detection:
///
/// - Sentences split at terminal punctuation `.`, `!`, `?`; a word is a
///   whitespace-separated token, bullets included.
/// - The split is deliberately naive: over-splitting decimals like `3.5`
///   or abbreviations like `e.g.` only shortens fragments, so it can
///   miss violations but never fabricate them.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for para in &doc.paragraphs {
        paragraph_diagnostics(para, &mut diags);
    }
    diags
}

/// Scans one paragraph's joined text and appends a diagnostic per
/// over-limit sentence, in one pass without allocating fragments.
fn paragraph_diagnostics(para: &Paragraph, diags: &mut Vec<Diagnostic>) {
    // Byte offset of the current sentence's first word; valid whenever
    // `words` is positive.
    let mut sentence_start = 0;
    let mut words = 0;
    // Whether the last char ended a word, so the next word char opens a
    // new word: true after whitespace or terminal punctuation.
    let mut at_word_start = true;
    // Index of the member line holding the current sentence's first word.
    let mut member = 0;
    // Whether the sentence just ended, so adjacent closers like `")` are
    // skipped instead of opening a new word.
    let mut after_terminal = false;

    for (offset, ch) in para.text.char_indices() {
        if after_terminal && CLOSERS.contains(&ch) {
            continue;
        }
        after_terminal = false;
        match ch {
            '.' | '!' | '?' => {
                finish_sentence(diags, para, sentence_start, words, &mut member);
                words = 0;
                at_word_start = true;
                after_terminal = true;
            }
            ch if ch.is_whitespace() => at_word_start = true,
            _ => {
                if at_word_start {
                    if words == 0 {
                        sentence_start = offset;
                    }
                    words += 1;
                    at_word_start = false;
                }
            }
        }
    }
    finish_sentence(diags, para, sentence_start, words, &mut member);
}

/// Appends the diagnostic for one finished sentence when `words` is over
/// the limit, advancing `member` to the sentence's start line.
fn finish_sentence(
    diags: &mut Vec<Diagnostic>,
    para: &Paragraph,
    sentence_start: usize,
    words: usize,
    member: &mut usize,
) {
    if words <= SENTENCE_LIMIT {
        return;
    }
    // The sentence begins on the last member line starting at or before
    // its first word; sentence starts only move forward through the text.
    while *member + 1 < para.line_starts.len() && para.line_starts[*member + 1].1 <= sentence_start
    {
        *member += 1;
    }
    diags.push(sentence_diagnostic(para.line_starts[*member].0, words));
}

/// TEXT003 Warning for one over-limit sentence, reported at its start
/// line with the comprehension research distilled.
fn sentence_diagnostic(line: usize, words: usize) -> Diagnostic {
    let bullets = [
        format!("Keep sentences to {SENTENCE_LIMIT} words or fewer."),
        "Long sentences can be harder to understand.".to_string(),
        format!(
            "Readers understand over 90% of the text when sentences \
             contain {SENTENCE_RECOMMENDED} words or fewer."
        ),
        "At 43 words per sentence, comprehension drops below 10%.".to_string(),
        "Split this sentence where the idea changes.".to_string(),
    ];
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_SENTENCE_LENGTH,
        message: bulleted(&format!("sentence is {words} words long."), &bullets),
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
    use indoc::formatdoc;

    /// `count` space-separated filler words, no terminator.
    fn words(count: usize) -> String {
        (0..count)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// One sentence of `count` words ending in `punct`.
    fn sentence(count: usize, punct: char) -> String {
        format!("{}{punct}", words(count))
    }

    // ── TEXT003: sentence length ──

    // Over-limit sentence -> TEXT003 Warning with a word-count summary
    // plus the word-limit guidance and the comprehension research.
    #[test]
    fn text_checks_warn_on_sentence_over_word_limit() {
        let source = format!("/// {}\n", sentence(SENTENCE_LIMIT + 1, '.'));
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.starts_with(&format!("sentence is {} words long.", SENTENCE_LIMIT + 1)));
        assert!(msg.contains(&format!("{SENTENCE_LIMIT} words or fewer")));
        assert!(msg.contains("harder to understand"));
        assert!(msg.contains("over 90%"));
        assert!(msg.contains("14 words or fewer"));
        assert!(msg.contains("drops below 10%"));
        assert!(msg.contains("where the idea changes"));
    }

    // A sentence of exactly `SENTENCE_LIMIT` words is at the limit, not
    // over it.
    #[test]
    fn text_checks_silent_on_sentence_at_word_limit() {
        let source = format!("/// {}\n", sentence(SENTENCE_LIMIT, '.'));
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_SENTENCE_LENGTH).is_empty());
    }

    // A final sentence still counts when it never receives terminal
    // punctuation, the common shape for doc comments and bullets.
    #[test]
    fn text_checks_warn_on_sentence_without_terminal_punctuation() {
        let source = format!("/// {}\n", words(SENTENCE_LIMIT + 1));
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
    }

    // Sentences split at `.`, `!`, and `?`: three 12-word sentences in
    // one 36-word paragraph stay silent where one sentence would fire.
    #[test]
    fn text_checks_split_sentences_at_terminal_punctuation() {
        let source = formatdoc! {"
            /// {}
            /// {}
            /// {}
        ", sentence(12, '!'), sentence(12, '?'), sentence(12, '.')};
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_SENTENCE_LENGTH).is_empty());
    }

    // The naive split is conservative by design: a decimal like `3.5`
    // splits a sentence four words over the limit into fragments under
    // it, so the rule stays silent rather than fabricating a violation.
    #[test]
    fn text_checks_stay_silent_when_a_decimal_splits_a_sentence() {
        let mut words: Vec<String> = (0..SENTENCE_LIMIT + 4).map(|i| format!("w{i}")).collect();
        words[9] = "3.5".to_string();
        let source = format!("/// {}.\n", words.join(" "));
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_SENTENCE_LENGTH).is_empty());
    }

    // An abbreviation like `e.g.` splits the same way: a sentence two
    // words over the limit shatters into `e`, `g`, and a final fragment
    // one word under it.
    #[test]
    fn text_checks_stay_silent_when_an_abbreviation_splits_a_sentence() {
        let source = format!("/// w0 e.g. {}.\n", words(SENTENCE_LIMIT - 1));
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_SENTENCE_LENGTH).is_empty());
    }

    // A sentence wrapped across lines reports at the line where it
    // begins, not where its words run out.
    #[test]
    fn text_checks_report_sentence_at_its_start_line() {
        let source = formatdoc! {"
            /// Intro sentence.
            /// {}
            /// {}.
        ", words(SENTENCE_LIMIT + 1), words(3)};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
    }

    // Bullet prose is measured at the same threshold, anchored at the
    // bullet's own line.
    #[test]
    fn text_checks_warn_on_over_limit_bullet_sentence() {
        let source = format!("- {}\n", sentence(SENTENCE_LIMIT + 1, '.'));
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
    }

    // Each over-limit sentence gets its own diagnostic, in source order.
    #[test]
    fn text_checks_warn_on_each_long_sentence_in_a_paragraph() {
        let source = formatdoc! {"
            /// {}
            /// {}
        ", sentence(SENTENCE_LIMIT + 1, '.'), sentence(SENTENCE_LIMIT + 1, '.')};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[1].line, 2);
        assert!(
            found[1]
                .message
                .starts_with(&format!("sentence is {} words long.", SENTENCE_LIMIT + 1))
        );
    }

    // Closers directly after terminal punctuation do not open a word, so
    // `.")` neither splits nor starts the next sentence.
    #[test]
    fn text_checks_stay_silent_when_closers_follow_terminal_punctuation() {
        let source = format!(
            "/// ({SENTENCE_LIMIT} words.) \"{}.\" Next one.\n",
            words(SENTENCE_LIMIT)
        );
        let diags = run_text_checks(&source, "rs");
        assert!(codes(&diags, CODE_SENTENCE_LENGTH).is_empty());
    }

    // After a closer, subsequent words count into the next sentence as
    // normal; only the adjacent closers are skipped.
    #[test]
    fn text_checks_count_words_after_closers_normally() {
        let source = format!(
            "/// (Short.) \"{})\" tail words here.\n",
            words(SENTENCE_LIMIT - 2)
        );
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .starts_with(&format!("sentence is {} words long.", SENTENCE_LIMIT + 1))
        );
    }

    // Headings and signature lines stay unmeasured upstream: their prose
    // never reaches a paragraph, as for TEXT001.
    #[test]
    fn text_checks_skip_headings_and_signature_lines() {
        let long = sentence(SENTENCE_LIMIT + 5, '.');
        let source = formatdoc! {"
            # {long}

            fn {long}
        "};
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_SENTENCE_LENGTH).is_empty());
    }
}
