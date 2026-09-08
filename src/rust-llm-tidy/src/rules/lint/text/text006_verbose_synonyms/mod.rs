//! TEXT006: suggest shorter wording without rewriting measured prose.
//!
//! # Layout
//!
//! - `suggestions` - the shorter-wording dictionary.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_VERBOSE_SYNONYMS;
use crate::text::measurement::{Document, StrippedLine};

mod suggestions;

/// Suggest shorter wording for the first match on each measured line.
///
/// # Matching
///
/// - Words: ASCII case-insensitive, with explicitly listed inflections only.
/// - Boundaries: alphanumeric and `_` neighbors extend a word.
/// - Phrases: whitespace separates words; punctuation and code interrupt them.
/// - Ties: the longest phrase at the earliest position wins.
/// - Exemptions: inline code spans and code blocks never fire.
///
/// # Remarks
///
/// Matching stays within a line; an open code span carries into the
/// region's next line. A line-number gap breaks the region and closes the
/// span.
///
/// Hints use the dictionary's lowercase wording and need a meaning and
/// grammar check; they never change the document.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut code_delimiter = 0;
    let mut previous_number = 0;
    for line in &doc.lines {
        if line.in_code_block {
            continue;
        }

        if line.number != previous_number + 1 {
            code_delimiter = 0;
        }
        previous_number = line.number;

        if let Some((before, after)) = first_suggestion(&line.text, &mut code_delimiter) {
            diags.push(synonym_diagnostic(line, before, after));
        }
    }
    diags
}

/// Find a hint without allocating tokens or a masked copy of the line.
///
/// Equal-length backtick runs open and close code spans. An unmatched opener
/// exempts the rest of the line and stays open in `code_delimiter` for the
/// caller's next line. The fixed dictionary bounds work per position.
fn first_suggestion(
    text: &str,
    code_delimiter: &mut usize,
) -> Option<(&'static str, &'static str)> {
    let mut chars = text.char_indices().peekable();
    let mut previous_is_word = false;
    while let Some((offset, ch)) = chars.next() {
        if ch == '`' {
            let mut run = 1;
            while chars.next_if(|&(_, next)| next == '`').is_some() {
                run += 1;
            }
            if *code_delimiter == 0 {
                *code_delimiter = run;
            } else if *code_delimiter == run {
                *code_delimiter = 0;
            }
            previous_is_word = false;
            continue;
        }

        if *code_delimiter == 0 && !previous_is_word && ch.is_ascii_alphabetic() {
            let initial = ch.to_ascii_lowercase() as u8;
            let entries = suggestions::SUGGESTIONS;
            let start = entries.partition_point(|(before, _)| before.as_bytes()[0] < initial);
            let matched = entries[start..]
                .iter()
                .take_while(|(before, _)| before.as_bytes()[0] == initial)
                .filter(|(before, _)| matches_phrase(&text[offset..], before))
                .max_by_key(|(before, _)| before.len());
            if let Some(&suggestion) = matched {
                return Some(suggestion);
            }
        }

        previous_is_word = is_word(ch);
    }
    None
}

/// Report dictionary wording and a possible rewrite, not an automatic fix.
fn synonym_diagnostic(line: &StrippedLine, before: &str, after: &str) -> Diagnostic {
    let bullets = [
        format!("Before: `{before}`"),
        format!("After: {after}"),
        "Preserve meaning and adjust grammar to fit.".to_string(),
        "Use the alternative only if it preserves technical meaning, uncertainty, and required wording.".to_string(),
    ];
    Diagnostic {
        severity: Severity::Hint,
        code: CODE_VERBOSE_SYNONYMS,
        message: bulleted(
            &format!("wording has a simpler alternative: `{before}`."),
            "Unnecessary formal wording and framing can make the point harder to understand.",
            &bullets,
        ),
        line: line.number,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// Match a dictionary phrase at the start of `text`, allowing variable whitespace.
///
/// Only outer punctuation is allowed, so phrases cannot cross sentence or code
/// boundaries. The caller checks the leading word boundary.
fn matches_phrase(mut text: &str, phrase: &str) -> bool {
    let mut words = phrase.split_ascii_whitespace().peekable();
    while let Some(word) = words.next() {
        let Some(prefix) = text.get(..word.len()) else {
            return false;
        };
        if !prefix.eq_ignore_ascii_case(word) {
            return false;
        }

        text = &text[word.len()..];
        if words.peek().is_some() {
            let rest = text.trim_start_matches(char::is_whitespace);
            if rest.len() == text.len() {
                return false;
            }
            text = rest;
        }
    }

    !text.starts_with(is_word)
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::tests::codes;
    use crate::rules::lint::{Dialect, DocRegion, RegionLine, run_region_checks, run_text_checks};

    // Dictionary and core behavior.

    // Every dictionary entry produces its own before/after hint at the source line.
    #[test]
    fn hints_should_show_before_and_after_for_every_entry() {
        for &(term, after) in suggestions::SUGGESTIONS {
            let source = format!("one\ntwo\nthree\nwe {term} this\n");

            let diags = run_text_checks(&source, "md");

            let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
            assert_eq!(found.len(), 1, "term {term}");
            assert_eq!(found[0].severity, Severity::Hint, "term {term}");
            assert_eq!(found[0].line, 4, "term {term}");
            assert_eq!(
                found[0].message,
                format!(
                    "wording has a simpler alternative: `{term}`.\n\
                     Why: Unnecessary formal wording and framing can make the point harder to understand.\n\
                     Suggestions:\n  - Before: `{term}`\n  \
                     - After: {after}\n  \
                     - Preserve meaning and adjust grammar to fit.\n  \
                     - Use the alternative only if it preserves technical meaning, uncertainty, and required wording."
                ),
                "term {term}"
            );
        }
    }

    // Sorted entries support initial-letter lookup without hiding any candidates.
    #[test]
    fn dictionary_should_keep_unique_sorted_entries() {
        for pair in suggestions::SUGGESTIONS.windows(2) {
            assert!(pair[0].0 < pair[1].0, "out of order or duplicate: {pair:?}");
        }
    }

    // Raw prose and explicit XML regions expose the same rendered hint messages.
    #[test]
    fn hints_should_match_across_text_and_region_entry_points() {
        let source = "We utilize this.\nDue to the fact that it failed, retry.\n";
        let regions = vec![DocRegion {
            dialect: Dialect::XmlDoc,
            lines: source
                .lines()
                .enumerate()
                .map(|(index, text)| RegionLine {
                    number: index + 1,
                    text: format!("<summary>{text}</summary>"),
                    indented: false,
                })
                .collect(),
        }];

        let text_diags = run_text_checks(source, "md");
        let region_diags = run_region_checks(regions);

        let text_hints = codes(&text_diags, CODE_VERBOSE_SYNONYMS);
        let region_hints = codes(&region_diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(text_hints.len(), 2);
        assert_eq!(text_hints, region_hints);
        assert_eq!(text_hints[0].line, 1);
        assert_eq!(text_hints[1].line, 2);
    }

    // Matching handles case, punctuation, whitespace, and explicit inflections.
    #[test]
    fn hints_should_match_when_case_or_word_boundaries_vary() {
        for (source, before, after) in [
            ("(Utilize) this.", "utilize", "`use`"),
            ("Use this;utilize that.", "utilize", "`use`"),
            ("Préface:utilize this.", "utilize", "`use`"),
            ("We do this In\tOrder  To help.", "in order to", "`to`"),
            ("We act prior\u{a0}to that.", "prior to", "`before`"),
            ("(In order to) help.", "in order to", "`to`"),
            ("She utilized this.", "utilized", "`used`"),
            ("She utilises this.", "utilises", "`uses`"),
            ("We are utilizing this.", "utilizing", "`using`"),
            (
                "It's worth noting that this works.",
                "it's worth noting that",
                "omit",
            ),
            ("Let’s delve into this.", "let’s delve into", "omit"),
        ] {
            let diags = run_text_checks(source, "md");

            let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
            assert_eq!(found.len(), 1, "{source}");
            assert!(
                found[0].message.contains(&format!("Before: `{before}`")),
                "{source}"
            );
            assert!(
                found[0].message.contains(&format!("After: {after}")),
                "{source}"
            );
        }
    }

    // Earliest source position wins, with longer phrases preferred at that position.
    #[test]
    fn hints_should_report_one_match_when_a_line_has_several() {
        for (source, expected) in [
            ("utilize this in order to demonstrate it", "utilize"),
            ("in order to utilize this", "in order to"),
            ("in close proximity to this", "in close proximity to"),
            (
                "as a result of the fact that it failed",
                "as a result of the fact that",
            ),
            (
                "it is important to note that we utilize this",
                "it is important to note that",
            ),
        ] {
            let diags = run_text_checks(source, "md");

            let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
            assert_eq!(found.len(), 1, "{source}");
            assert!(
                found[0].message.contains(&format!("Before: `{expected}`")),
                "{source}"
            );
        }
    }

    // Technical words, unlisted forms, and interrupted phrases stay silent.
    #[test]
    fn hints_should_stay_silent_when_no_whole_entry_matches() {
        for source in [
            "demonstration and facilitation stay.",
            "re_utilize and demonstrate_x stay.",
            "éutilize utilizeé 2utilize utilize2 stay.",
            "implement execute allocate serialize stay.",
            "in order\nto help",
            "in order. To help",
            "in, order to help",
            "in order `example` to help",
            "in order `` to help",
            "in order today",
            "within order to help",
            "in order to_do this",
            "in order toé help",
            "in order très bien",
        ] {
            let diags = run_text_checks(source, "md");

            assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty(), "{source}");
        }
    }

    // Code exemptions and comment entry points.

    // Backtick run length controls the exemption, including dangling openers.
    #[test]
    fn hints_should_stay_silent_when_terms_are_inside_code_spans() {
        for source in [
            "call `utilize(x)` and `in order to` helpers.",
            "call ``utilize ` in order to`` helpers.",
            "call ```utilize `` in order to``` helpers.",
            "call `utilize this",
            "call ``utilize ` in order to",
            "call `utilize `` in order to` helpers.",
        ] {
            let diags = run_text_checks(source, "md");

            assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty(), "{source}");
        }
    }

    // Closing a span resumes prose matching without exposing span contents.
    #[test]
    fn hints_should_resume_when_a_code_span_closes() {
        for source in [
            "`utilize` prior to this",
            "``utilize ` this`` prior to this",
            "call ```utilize `` this``` prior to this",
        ] {
            let diags = run_text_checks(source, "md");

            let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
            assert_eq!(found.len(), 1, "{source}");
            assert!(found[0].message.contains("Before: `prior to`"), "{source}");
        }
    }

    // A span left open at line end carries into the region's next line.
    #[test]
    fn hints_should_stay_silent_when_a_span_spans_lines() {
        let source = "call `utilize this\nin order to` helpers.";

        let diags = run_text_checks(source, "md");

        assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty());
    }

    // A span closed on a later line resumes prose matching on that line.
    #[test]
    fn hints_should_resume_when_a_span_closes_on_a_later_line() {
        let source = "code `span stays\nopen` prior to this";

        let diags = run_text_checks(source, "md");

        let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert!(found[0].message.contains("Before: `prior to`"));
    }

    // A non-doc line ends the region: span state does not carry into the
    // next region's lines.
    #[test]
    fn hints_should_reset_span_state_when_a_region_breaks() {
        let source = "/// `utilize this\nlet x = 1;\n/// utilize this\n";

        let diags = run_text_checks(source, "rs");

        let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    // Both block forms are exempt in markdown and Rust doc comments.
    #[test]
    fn hints_should_stay_silent_when_terms_are_inside_code_blocks() {
        for (source, ext) in [
            ("```text\nutilize this\n```\n\n    demonstrate that\n", "md"),
            ("///     utilize this\n", "rs"),
        ] {
            let diags = run_text_checks(source, ext);

            assert!(codes(&diags, CODE_VERBOSE_SYNONYMS).is_empty(), "{source}");
        }
    }

    // Rust doc comments report the source line and concrete rewrite guidance.
    #[test]
    fn hints_should_show_before_and_after_when_in_rust_doc_comments() {
        let source = "/// We utilize this helper.\n";

        let diags = run_text_checks(source, "rs");

        let found = codes(&diags, CODE_VERBOSE_SYNONYMS);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert!(found[0].message.contains("Before: `utilize`"));
        assert!(found[0].message.contains("After: `use`"));
    }
}
