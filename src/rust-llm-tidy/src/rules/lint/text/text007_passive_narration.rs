//! TEXT007: passive constructions and past-behavior narration in
//! comments and docs.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_PASSIVE_NARRATION;
use crate::text::measurement::is_link_reference_definition;
use crate::text::measurement::{Document, StrippedLine};

/// Participles accepted as adjectives after a be-verb; `is required` and
/// `is deprecated` describe state, not voice.
const ADJECTIVAL_PARTICIPLES: &[&str] = &["required", "deprecated"];
/// Be-verbs whose immediate participle neighbor marks a passive voice.
const BE_VERBS: &[&str] = &["is", "are", "was", "were", "be", "been", "being"];
/// Valid participles ending in `en`; bare `en` words such as `open` and
/// `ten` are state adjectives or nouns, not passives.
const EN_PARTICIPLES: &[&str] = &[
    "beaten",
    "broken",
    "chosen",
    "driven",
    "eaten",
    "fallen",
    "forgotten",
    "given",
    "hidden",
    "proven",
    "rewritten",
    "risen",
    "taken",
    "written",
];
/// Participles that do not end in `ed` or `en` but still form passives.
const IRREGULAR_PARTICIPLES: &[&str] = &[
    "built", "brought", "caught", "held", "kept", "left", "made", "met", "paid", "put", "run",
    "said", "sent", "set", "taught", "told",
];
/// Word-bounded narration phrases checked before single-word markers.
const NARRATION_MARKERS: &[NarrationMarker] = &[
    NarrationMarker {
        tokens: &["no", "longer"],
        display: "no longer",
    },
    NarrationMarker {
        tokens: &["used", "to"],
        display: "used to",
    },
    NarrationMarker {
        tokens: &["in", "the", "past"],
        display: "in the past",
    },
];
/// Summary prefix carried by every narration-marker diagnostic; the
/// passive class opens with `passive construction:` instead.
const NARRATION_MARKER_SUMMARY: &str = "past-behavior narration marker: ";
/// History and change-relative wording, including redundant present-time labels.
const SINGLE_WORD_NARRATION_MARKERS: &[&str] = &[
    "previously",
    "now",
    "formerly",
    "historically",
    "originally",
    "recently",
    "lately",
    "currently",
    "anymore",
];

/// One narration marker: its token sequence and display form.
struct NarrationMarker {
    tokens: &'static [&'static str],
    display: &'static str,
}

/// TEXT007 diagnostics for `doc`: at most one Warning per measured line,
/// in source order.
///
/// Finding classes share the code:
///
/// - Passive: a be-verb followed by a past participle, except accepted
///   adjectival participles.
/// - Narration marker: a phrase in [`NARRATION_MARKERS`], a word in
///   [`SINGLE_WORD_NARRATION_MARKERS`], bare `was`, or clause-initial `before,`.
///
/// A line matching both classes reports the passive class only.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in &doc.lines {
        if line.in_code_block {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.is_empty() || trimmed.starts_with('|') || is_link_reference_definition(trimmed) {
            continue;
        }
        if let Some(summary) = find_offense(trimmed) {
            diags.push(diagnostic(line, &summary));
        }
    }
    diags
}

/// Whether `diag` is a TEXT007 narration-marker finding.
///
/// Callers with the checked file's path (the pipeline lint pass) use
/// this to suppress marker findings in release and migration notes;
/// passive findings still fire there. This module never sees paths.
pub(crate) fn is_narration_marker(diag: &Diagnostic) -> bool {
    diag.code == CODE_PASSIVE_NARRATION && diag.message.starts_with(NARRATION_MARKER_SUMMARY)
}

/// One TEXT007 Warning; `summary` names the finding class and trigger.
fn diagnostic(line: &StrippedLine, summary: &str) -> Diagnostic {
    let bullets = [
        "State only current behavior in active, present-tense language.".to_string(),
        "Remove change history, old/new comparisons, and time labels such as `now` or `currently`."
             .to_string(),
        "Delete history-only sentences; do not invent replacement behavior."
            .to_string(),
        "Check the implementation before rewriting; preserve exact conditions, guarantees, and limitations."
            .to_string(),
        "Keep history out of comments and API docs, including internals, tests, and helpers. \
         Use release or migration notes only for a genuine public-API compatibility concern."
            .to_string(),
    ];

    Diagnostic {
        severity: Severity::Warning,
        code: CODE_PASSIVE_NARRATION,
        message: bulleted(summary, &bullets),
        line: line.number,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// The single-offense summary for one line, passive class first.
///
/// Returns `None` when the line holds neither a passive construction nor
/// a narration marker.
fn find_offense(line: &str) -> Option<String> {
    let words = alphabetic_words(line);
    if let Some((be, participle)) = find_passive(&words) {
        return Some(format!(
            "passive construction: `{} {}`.",
            be.to_ascii_lowercase(),
            participle.to_ascii_lowercase()
        ));
    }
    if let Some(marker) = find_narration_marker(line, &words) {
        return Some(format!("{NARRATION_MARKER_SUMMARY}`{marker}`."));
    }
    None
}

/// Alphabetic runs of `line`, in order, compared case-insensitively by
/// the matchers so no lowercase copies are allocated.
fn alphabetic_words(line: &str) -> Vec<&str> {
    line.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The first narration marker in the line, if any.
///
/// Phrase markers take precedence over single words. Bare `was` is the
/// fallback; clause-initial `before,` excludes temporal `before validation`.
fn find_narration_marker(line: &str, words: &[&str]) -> Option<&'static str> {
    for marker in NARRATION_MARKERS {
        if words.windows(marker.tokens.len()).any(|w| {
            w.iter()
                .zip(marker.tokens)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        }) {
            return Some(marker.display);
        }
    }

    if let Some(marker) = SINGLE_WORD_NARRATION_MARKERS
        .iter()
        .find(|marker| words.iter().any(|word| word.eq_ignore_ascii_case(marker)))
    {
        return Some(marker);
    }

    if has_clause_initial_before_comma(line) {
        return Some("before,");
    }

    words
        .iter()
        .any(|w| w.eq_ignore_ascii_case("was"))
        .then_some("was")
}

/// A be-verb immediately followed by a past participle, if any.
///
/// Adjectival participles such as `required` and `deprecated` are
/// accepted and never fire.
fn find_passive<'a>(words: &'a [&'a str]) -> Option<(&'a str, &'a str)> {
    words.windows(2).find_map(|pair| {
        matches_any(pair[0], BE_VERBS)
            .then(|| pair[1])
            .filter(|next| is_participle(next))
            .map(|next| (pair[0], next))
    })
}

/// Whether `line` contains `before,` at a line, sentence, or clause
/// start, matched ASCII-case-insensitively without copying the line.
fn has_clause_initial_before_comma(line: &str) -> bool {
    let mut rest = line;
    while let Some(pos) = find_before_comma_ci(rest) {
        if is_clause_start(&rest[..pos]) {
            return true;
        }
        rest = &rest[pos + "before,".len()..];
    }
    false
}

/// Whether `word` is a past-participle form.
fn is_participle(word: &str) -> bool {
    if matches_any(word, ADJECTIVAL_PARTICIPLES) {
        return false;
    }
    ends_with_ci(word, "ed")
        || matches_any(word, EN_PARTICIPLES)
        || matches_any(word, IRREGULAR_PARTICIPLES)
}

/// Whether `word` ends with `suffix`, ASCII-case-insensitively.
///
/// ASCII-only words keep the byte suffix slice on a char boundary.
fn ends_with_ci(word: &str, suffix: &str) -> bool {
    word.is_ascii()
        && word.len() > suffix.len()
        && word[word.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Byte offset of the first ASCII-case-insensitive `before,` in `line`.
fn find_before_comma_ci(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let needle = b"before,";
    bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(i, _)| line.is_char_boundary(*i))
        .find(|(_, w)| w.eq_ignore_ascii_case(needle))
        .map(|(i, _)| i)
}

/// Whether the text before one `before,` occurrence ends a clause, so
/// the marker itself starts a new clause.
fn is_clause_start(prefix: &str) -> bool {
    let trimmed = prefix.trim().trim_start_matches(['-', '*', '+', '>', '#']);
    trimmed.is_empty()
        || trimmed
            .chars()
            .last()
            .is_some_and(|c| matches!(c, '.' | '!' | '?' | ';' | ':'))
}

/// Whether `word` equals any entry of `words`, ASCII-case-insensitively.
fn matches_any(word: &str, words: &[&str]) -> bool {
    words.iter().any(|known| word.eq_ignore_ascii_case(known))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use indoc::formatdoc;

    /// TEXT007 findings for one measured markdown prose line.
    fn one_line(line: &str) -> Vec<crate::reporting::Diagnostic> {
        let diags = run_text_checks(&format!("{line}\n"), "md");
        codes(&diags, CODE_PASSIVE_NARRATION)
            .into_iter()
            .cloned()
            .collect()
    }

    // ── TEXT007: passive constructions ──

    // Each card fire example with a be-verb plus participle warns once.
    #[test]
    fn text_checks_warn_on_passive_fire_examples() {
        for (source, pair) in [
            ("Errors are returned by the scanner.", "are returned"),
            ("The value was parsed by the loader.", "was parsed"),
            ("Links will be rewritten.", "be rewritten"),
        ] {
            let found = one_line(source);
            assert_eq!(found.len(), 1, "{source:?}");
            assert_eq!(found[0].severity, Severity::Warning);
            assert_eq!(found[0].line, 1);
            assert!(
                found[0]
                    .message
                    .starts_with(&format!("passive construction: `{pair}`.")),
                "{}",
                found[0].message
            );
        }
    }

    // Each card preferred active fix stays silent.
    #[test]
    fn text_checks_silent_on_active_fix_examples() {
        for source in [
            "The scanner returns errors.",
            "The loader parses the value.",
            "The pass rewrites links.",
            "This panics only on empty input.",
        ] {
            assert!(one_line(source).is_empty(), "{source:?}");
        }
    }

    // Accepted adjectival participles describe state, not voice.
    #[test]
    fn text_checks_silent_on_adjectival_participles() {
        assert!(one_line("The flag is required for streaming.").is_empty());
        assert!(one_line("This method is deprecated.").is_empty());
    }

    // Bare `en` words are state adjectives or nouns, never passives.
    #[test]
    fn text_checks_silent_on_non_participle_en_words() {
        assert!(one_line("The door is open.").is_empty());
        assert!(one_line("The count is ten.").is_empty());
        assert!(one_line("The word is often misspelled.").is_empty());
    }

    // Controlled `en` participles after a be-verb still warn.
    #[test]
    fn text_checks_warn_on_en_participles() {
        let found = one_line("The report was written by the tool.");
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .starts_with("passive construction: `was written`.")
        );
    }

    // The passive summary quotes the matched be-verb and participle.
    #[test]
    fn text_checks_names_the_matched_passive_pair() {
        let found = one_line("The errors were returned by the scanner.");
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .starts_with("passive construction: `were returned`.")
        );
    }

    // ── TEXT007: narration markers ──

    // Each single-word and phrase marker warns once, naming the marker.
    #[test]
    fn text_checks_should_name_marker_when_prose_narrates_behavior() {
        for (source, marker) in [
            ("This no longer panics.", "no longer"),
            ("The old path previously ran here.", "previously"),
            ("This flag used to default on.", "used to"),
            ("The cache is now bounded.", "now"),
            ("The value was large.", "was"),
            (
                "In the past, the cache grew without a bound.",
                "in the past",
            ),
            ("The parser formerly accepted empty names.", "formerly"),
            (
                "Historically, the parser accepted empty names.",
                "historically",
            ),
            ("The parser originally accepted empty names.", "originally"),
            ("The parser recently gained a size limit.", "recently"),
            ("Lately, the parser rejects empty names.", "lately"),
            ("The parser currently rejects empty names.", "currently"),
            ("The parser does not accept empty names anymore.", "anymore"),
            ("FORMERLY, the cache grew without a bound.", "formerly"),
            (
                "IN THE PAST, the cache grew without a bound.",
                "in the past",
            ),
        ] {
            let found = one_line(source);

            assert_eq!(found.len(), 1, "{source:?}");
            assert_eq!(found[0].severity, Severity::Warning);
            assert!(is_narration_marker(&found[0]), "{source:?}");
            assert!(
                found[0]
                    .message
                    .starts_with(&format!("past-behavior narration marker: `{marker}`.")),
                "{}",
                found[0].message
            );
        }
    }

    // Clause-initial `Before,` fires; temporal `before` never does.
    #[test]
    fn text_checks_warn_on_clause_initial_before_only() {
        let found = one_line("Before, the pass rewrote links.");
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .starts_with("past-behavior narration marker: `before,`.")
        );
        assert!(one_line("Validate input before validation runs.").is_empty());
        assert!(one_line("Rewrite links before scanning.").is_empty());
    }

    // Embedded markers and general temporal vocabulary do not imply change history.
    #[test]
    fn text_checks_should_stay_silent_when_words_describe_current_behavior() {
        for source in [
            "Document the known anchors.",
            "The swap module bounds the cache.",
            "Workers run concurrently.",
            "The handler runs once per request.",
            "Copy the old value into the new buffer.",
            "Reject timestamps in the future or past.",
            "Read the previous entry before validation.",
            "Return the most recent entry.",
        ] {
            let found = one_line(source);

            assert!(found.is_empty(), "{source:?}");
        }
    }

    // Non-ASCII words after a be-verb never panic and never match.
    #[test]
    fn text_checks_silent_on_non_ascii_words() {
        assert!(one_line("The glyph is 中.").is_empty());
        assert!(one_line("中文 is the label.").is_empty());
    }

    // ── TEXT007: one diagnostic per line and message shape ──

    // A line matching both classes yields one diagnostic, passive class.
    #[test]
    fn text_checks_emit_one_diagnostic_per_line_preferring_passive() {
        let found = one_line("Errors are returned by the scanner now.");
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .starts_with("passive construction: `are returned`.")
        );
    }

    // Both finding classes guide the same current-behavior rewrite.
    #[test]
    fn text_checks_should_explain_current_behavior_rewrite_when_reporting() {
        let expected = concat!(
            "\n  - State only current behavior in active, present-tense language.",
            "\n  - Remove change history, old/new comparisons, ",
            "and time labels such as `now` or `currently`.",
            "\n  - Delete history-only sentences; do not invent replacement behavior.",
            "\n  - Check the implementation before rewriting; ",
            "preserve exact conditions, guarantees, and limitations.",
            "\n  - Keep history out of comments and API docs, ",
            "including internals, tests, and helpers.",
            " Use release or migration notes only for a genuine ",
            "public-API compatibility concern.",
        );

        for source in [
            "Errors are returned by the scanner.",
            "The scanner formerly accepted empty names.",
        ] {
            let found = one_line(source);

            assert_eq!(found.len(), 1, "{source:?}");
            assert!(found[0].message.ends_with(expected), "{}", found[0].message);
        }
    }

    // Comment lines and multi-line docs warn per measured source line.
    #[test]
    fn text_checks_warn_per_measured_line_in_comments() {
        let source = formatdoc! {"
            /// Parses the input.
            ///
            /// Errors are returned by the scanner.
            ///
            ///     Errors are returned by the scanner.
            ///
            /// This no longer panics.
        "};
        let diags = run_text_checks(&source, "rs");
        let found = codes(&diags, CODE_PASSIVE_NARRATION);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].line, 3);
        assert!(found[0].message.starts_with("passive construction:"));
        assert_eq!(found[1].line, 7);
        assert!(
            found[1]
                .message
                .starts_with("past-behavior narration marker:")
        );
    }
}
