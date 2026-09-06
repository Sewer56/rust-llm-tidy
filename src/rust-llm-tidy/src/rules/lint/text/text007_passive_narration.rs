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
/// Participles that do not end in `ed` or `en` but still form passives.
const IRREGULAR_PARTICIPLES: &[&str] = &[
    "built", "brought", "caught", "held", "kept", "left", "made", "met", "paid", "put", "run",
    "said", "sent", "set", "taught", "told",
];
/// Past-behavior narration markers, word-bounded, checked before the
/// bare-`was` fallback.
const NARRATION_MARKERS: &[NarrationMarker] = &[
    NarrationMarker {
        tokens: &["no", "longer"],
        display: "no longer",
    },
    NarrationMarker {
        tokens: &["used", "to"],
        display: "used to",
    },
];
/// Summary prefix carried by every narration-marker diagnostic; the
/// passive class opens with `passive construction:` instead.
const NARRATION_MARKER_SUMMARY: &str = "past-behavior narration marker: ";

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
/// - Narration marker: `no longer`, `previously`, `used to`, `now`, bare
///   `was`, or clause-initial `before,`.
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
/// this to suppress marker findings in release and migration notes
/// while passive findings still fire there. This module never sees
/// paths itself.
pub(crate) fn is_narration_marker(diag: &Diagnostic) -> bool {
    diag.code == CODE_PASSIVE_NARRATION && diag.message.starts_with(NARRATION_MARKER_SUMMARY)
}

/// One TEXT007 Warning; `summary` names the finding class and trigger.
fn diagnostic(line: &StrippedLine, summary: &str) -> Diagnostic {
    let bullets = [
        "State the current contract in active voice.".to_string(),
        "Old behavior belongs in release or migration notes only for a \
         genuine public-API compatibility concern."
            .to_string(),
        "Confirm that obligation with the user rather than narrating.".to_string(),
        "Private code, internals, tests, and helpers never carry old behavior.".to_string(),
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
/// Bare `was` is the fallback marker; clause-initial `before,` requires
/// a line or clause start, so temporal `before validation` never fires.
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
    if words.iter().any(|w| w.eq_ignore_ascii_case("previously")) {
        return Some("previously");
    }
    if words.iter().any(|w| w.eq_ignore_ascii_case("now")) {
        return Some("now");
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
    ends_with_ci(word, "ed") || ends_with_ci(word, "en") || matches_any(word, IRREGULAR_PARTICIPLES)
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
    fn text_checks_warn_on_each_narration_marker() {
        for (source, marker) in [
            ("This no longer panics.", "no longer"),
            ("The old path previously ran here.", "previously"),
            ("This flag used to default on.", "used to"),
            ("The cache is now bounded.", "now"),
            ("The value was large.", "was"),
        ] {
            let found = one_line(source);
            assert_eq!(found.len(), 1, "{source:?}");
            assert_eq!(found[0].severity, Severity::Warning);
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

    // Word boundaries keep embedded `now` and `was` silent.
    #[test]
    fn text_checks_silent_on_embedded_marker_words() {
        assert!(one_line("Document the known anchors.").is_empty());
        assert!(one_line("The swap module bounds the cache.").is_empty());
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

    // The guidance carries the card message, bulleted after the summary.
    #[test]
    fn text_checks_message_carries_remediation_guidance() {
        let found = one_line("Errors are returned by the scanner.");
        let msg = &found[0].message;
        assert!(msg.contains("State the current contract in active voice."));
        assert!(msg.contains("genuine public-API compatibility concern"));
        assert!(msg.contains("Confirm that obligation with the user"));
        assert!(msg.contains("Private code, internals, tests, and helpers never carry old"));
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
