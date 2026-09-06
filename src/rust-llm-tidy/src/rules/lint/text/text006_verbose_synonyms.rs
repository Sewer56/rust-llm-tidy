//! TEXT006: verbose synonyms over the plaintext analysis.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_VERBOSE_SYNONYMS;
use crate::text::measurement::{Document, StrippedLine};

/// The discouraged single words, lowercased for comparison.
const WORDS: &[&str] = &["utilize", "demonstrate", "facilitate"];

/// TEXT006 diagnostics for `doc`: one Warning per measured line that
/// matches a term (the first match), in source order.
///
/// Matching per line:
///
/// - Case-insensitive whole-word matches only: `utilizes`,
///   `utilization`, and `demonstration` never fire, `Utilize` does.
/// - Word boundaries follow the `contains_word` precedent: an
///   alphanumeric or `_` neighbor extends the word, punctuation
///   neighbors do not.
/// - The phrase is three consecutive whole words separated by
///   whitespace within one line; it never matches across lines.
/// - Occurrences inside inline code spans (backtick-delimited runs)
///   are exempt. Fenced and indented code blocks are already
///   unmeasured.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in &doc.lines {
        if line.in_code_block {
            continue;
        }

        let masked = mask_code_spans(&line.text);
        let mut matched: Option<String> = None;
        let words = masked.split_whitespace().map(trim_word_edges);
        let mut previous_two: [String; 2] = [String::new(), String::new()];
        for word in words {
            let lower = word.to_ascii_lowercase();
            if matched.is_none() && WORDS.contains(&lower.as_str()) {
                matched = Some(lower.clone());
            }
            if matched.is_none() && previous_two == ["in", "order"] && lower == "to" {
                matched = Some("in order to".to_string());
            }
            previous_two = [previous_two[1].clone(), lower];
        }

        if let Some(term) = matched {
            diags.push(synonym_diagnostic(line, &term));
        }
    }
    diags
}

/// `text` with every backtick-delimited run (delimiters included)
/// replaced by spaces, so span contents never match.
///
/// An unmatched opening backtick masks the rest of the line: text
/// after a dangling delimiter is treated as span content, never prose.
fn mask_code_spans(text: &str) -> String {
    let mut masked = String::with_capacity(text.len());
    let mut in_span = false;
    for ch in text.chars() {
        if ch == '`' {
            in_span = !in_span;
            masked.push(' ');
        } else {
            masked.push(if in_span { ' ' } else { ch });
        }
    }
    masked
}

/// TEXT006 Warning for one matched `term`, reported at its line.
fn synonym_diagnostic(line: &StrippedLine, term: &str) -> Diagnostic {
    let bullets = [
        "Pick the short everyday word: use, show, help, or so that.".to_string(),
        "Inline code spans are exempt; quoted code never fires.".to_string(),
    ];
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_VERBOSE_SYNONYMS,
        message: bulleted(&format!("verbose synonym: {term}."), &bullets),
        line: line.number,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// One whitespace token with non-word edge characters removed.
///
/// Word characters for boundary purposes are alphanumerics and `_`,
/// matching the `contains_word` precedent: `utilize,` and `(utilize)`
/// trim to `utilize`, `re_utilize` stays one longer word.
fn trim_word_edges(token: &str) -> &str {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    token
        .trim_start_matches(|c| !is_word(c))
        .trim_end_matches(|c| !is_word(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use indoc::formatdoc;

    // ── Detection: each term fires ──

    // Each discouraged term fires one TEXT006 Warning at its line
    // through the text-rule entry path.
    #[test]
    fn text_checks_warn_on_each_verbose_synonym() {
        for term in ["utilize", "demonstrate", "facilitate"] {
            let source = format!("one\ntwo\nthree\nwe {term} this\n");
            let diags = run_text_checks(&source, "md");
            let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
            assert_eq!(found.len(), 1, "term {term}");
            assert_eq!(found[0].severity, Severity::Warning, "term {term}");
            assert_eq!(found[0].line, 4, "term {term}");
            assert!(
                found[0].message.contains(term),
                "message names {term}: {}",
                found[0].message
            );
        }
    }

    // The phrase fires as three consecutive whole words.
    #[test]
    fn text_checks_warn_on_in_order_to_phrase() {
        let source = "we do this In Order To help\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        assert!(found[0].message.contains("in order to"));
    }

    // ── Whole-word boundaries and case ──

    // Longer words containing the terms never fire.
    #[test]
    fn text_checks_silent_on_longer_word_forms() {
        let source = "utilizes, utilization, and demonstration stay.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty());
    }

    // A capitalized term still fires; punctuation neighbors keep the
    // whole-word match.
    #[test]
    fn text_checks_warn_on_capitalized_and_punctuated_term() {
        let source = "Utilize it (utilize) again.\n";
        let diags = run_text_checks(source, "md");
        assert_eq!(codes(&diags, CODE_VERBOSE_SYNONYMS).len(), 1);
    }

    // An underscore neighbor extends the word, so it does not fire.
    #[test]
    fn text_checks_silent_on_underscore_extended_words() {
        let source = "the re_utilize and demonstrate_x forms.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty());
    }

    // ── Inline code spans and code blocks ──

    // A term inside an inline code span does not fire.
    #[test]
    fn text_checks_silent_inside_inline_code_span() {
        let source = "call `utilize(x)` and `in order to` helpers.\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty());
    }

    // Terms in fenced and indented code blocks never fire; prose
    // around them still does.
    #[test]
    fn text_checks_silent_inside_code_blocks() {
        let fenced = formatdoc! {"
            ```
            utilize this
            ```

                demonstrate that
        "};
        assert!(codes(&run_text_checks(&fenced, "md"), CODE_VERBOSE_SYNONYMS).is_empty());
        let indented_rs = "///     utilize this\n";
        assert!(codes(&run_text_checks(indented_rs, "rs"), CODE_VERBOSE_SYNONYMS).is_empty());
    }

    // A doc comment in Rust fires through the same entry path.
    #[test]
    fn text_checks_warn_on_rust_doc_comment() {
        let source = "/// We utilize this helper.\n";
        let diags = run_text_checks(source, "rs");
        let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    // Matching is per line: a phrase split across two lines never
    // fires.
    #[test]
    fn text_checks_silent_on_phrase_split_across_lines() {
        let source = "in order\nto do this\n";
        let diags = run_text_checks(source, "md");
        assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty());
    }

    // A line matching several terms still yields exactly one
    // diagnostic: the first match wins.
    #[test]
    fn text_checks_emit_one_diagnostic_per_line_with_several_terms() {
        let source = "utilize this in order to demonstrate it\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("utilize"));
    }
}
