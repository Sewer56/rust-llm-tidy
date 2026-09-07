//! Suggest clearer wording for passive voice and implementation history.
//!
//! # How it works
//!
//! - Read prose line by line; skip code blocks, tables, and link definitions.
//! - Collect words outside inline code and link targets, keeping their positions.
//! - Look for a be-verb (`is`, `are`, `was`) followed by a likely past
//!   participle (`returned`, `parsed`, `written`), such as `are returned`.
//! - Otherwise, look for history wording, such as `before this change`.
//! - Emit at most one hint per line; passive voice takes priority over history.
//!
//! # Exceptions
//!
//! Passive voice stays silent for:
//!
//! - Accepted adjectival participles: `required`, `deprecated`, `unnamed`,
//!   and `outdated`.
//! - State participles such as `sorted`, `selected`, `based on`, and
//!   `concerned`, unless immediately followed by `by`.
//! - Non-participles such as `red`, `seed`, and bare `open` or `ten`.
//! - Relational `related to` and modal `supposed to`.
//! - Compound auxiliaries glued to a hyphen, such as `soon-to-be removed`.
//! - Conventional license wording: `licensed under`, `distributed in the
//!   hope that`, `provided as is`, and `permitted provided`.
//!
//! Narration markers stay silent for:
//!
//! - Determiner modifiers such as `the currently selected item`, plus the
//!   hyphenated `least-recently-used` cache policy.
//! - Runtime states such as `currently active` and `currently running`.
//! - Attributive present-time modifiers such as `now removed features` and
//!   `very recently loaded data`.
//! - Present intent: `for now`, `now to <verb>`, `We now need`, and a bare
//!   value after `currently`, as in `currently 0.1.157`.
//! - `no longer` before `than`, and temporal `before` as in
//!   `before validation`; only clause-initial `Before,` matches.
//!
//! # Remarks
//!
//! - This heuristic has no grammatical context: it is prone to false
//!   positives, its hints need human judgment, and it never rewrites source.
//! - Punctuation interrupts phrases; matches are case-insensitive, and
//!   digits and underscores extend a word.
//! - Inline code, link targets, reference labels, autolinks, and HTTP URLs
//!   stay opaque; backtick code spans carry across consecutive prose lines.
//! - File processing runs this rule only when opted in: the config's
//!   `passive_narration.enable` setting or an explicit `TEXT007` inclusion.
//!   Narration markers stay suppressed in release notes by default.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_PASSIVE_NARRATION;
use crate::text::measurement::is_link_reference_definition;
use crate::text::measurement::{Document, StrippedLine};

mod prose;

/// Participles accepted as adjectives after a be-verb; `is required` and
/// `is deprecated` describe state, not voice.
const ADJECTIVAL_PARTICIPLES: &[&str] = &["required", "deprecated", "unnamed", "outdated"];
/// Be-verbs whose immediate participle neighbor marks a passive voice.
const BE_VERBS: &[&str] = &["is", "are", "was", "were", "be", "been", "being"];
/// Ambiguous suffix matches commonly name adjectives, nouns, or base verbs.
const ED_NON_PARTICIPLES: &[&str] = &[
    "bed", "bleed", "breed", "creed", "feed", "greed", "hundred", "indeed", "naked", "need", "red",
    "reed", "sacred", "seed", "shed", "shred", "speed", "steed", "tweed", "wed", "weed", "wicked",
];
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
/// Longer phrases precede their contained phrases for more specific diagnostics.
const NARRATION_MARKERS: &[NarrationMarker] = &[
    NarrationMarker {
        tokens: &["no", "longer"],
        display: "no longer",
    },
    NarrationMarker {
        tokens: &["in", "the", "past"],
        display: "in the past",
    },
    NarrationMarker {
        tokens: &["prior", "to", "this", "change"],
        display: "prior to this change",
    },
    NarrationMarker {
        tokens: &["before", "this", "change"],
        display: "before this change",
    },
    NarrationMarker {
        tokens: &["after", "this", "change"],
        display: "after this change",
    },
    NarrationMarker {
        tokens: &["with", "this", "change"],
        display: "with this change",
    },
    NarrationMarker {
        tokens: &["previous", "implementation"],
        display: "previous implementation",
    },
    NarrationMarker {
        tokens: &["old", "implementation"],
        display: "old implementation",
    },
    NarrationMarker {
        tokens: &["earlier", "versions"],
        display: "earlier versions",
    },
    NarrationMarker {
        tokens: &["previous", "versions"],
        display: "previous versions",
    },
    NarrationMarker {
        tokens: &["in", "earlier", "releases"],
        display: "in earlier releases",
    },
    NarrationMarker {
        tokens: &["in", "previous", "releases"],
        display: "in previous releases",
    },
    NarrationMarker {
        tokens: &["in", "prior", "releases"],
        display: "in prior releases",
    },
    NarrationMarker {
        tokens: &["earlier", "implementation"],
        display: "earlier implementation",
    },
    NarrationMarker {
        tokens: &["prior", "implementation"],
        display: "prior implementation",
    },
    NarrationMarker {
        tokens: &["original", "implementation"],
        display: "original implementation",
    },
    NarrationMarker {
        tokens: &["earlier", "behavior"],
        display: "earlier behavior",
    },
    NarrationMarker {
        tokens: &["previous", "behavior"],
        display: "previous behavior",
    },
    NarrationMarker {
        tokens: &["prior", "behavior"],
        display: "prior behavior",
    },
    NarrationMarker {
        tokens: &["old", "behavior"],
        display: "old behavior",
    },
    NarrationMarker {
        tokens: &["earlier", "behaviour"],
        display: "earlier behaviour",
    },
    NarrationMarker {
        tokens: &["previous", "behaviour"],
        display: "previous behaviour",
    },
    NarrationMarker {
        tokens: &["prior", "behaviour"],
        display: "prior behaviour",
    },
    NarrationMarker {
        tokens: &["old", "behaviour"],
        display: "old behaviour",
    },
    NarrationMarker {
        tokens: &["before", "this", "fix"],
        display: "before this fix",
    },
    NarrationMarker {
        tokens: &["after", "this", "fix"],
        display: "after this fix",
    },
    NarrationMarker {
        tokens: &["with", "this", "fix"],
        display: "with this fix",
    },
    NarrationMarker {
        tokens: &["as", "of", "this", "release"],
        display: "as of this release",
    },
    NarrationMarker {
        tokens: &["as", "of", "this", "version"],
        display: "as of this version",
    },
];
/// Summary prefix carried by every narration-marker diagnostic; the
/// passive class opens with `passive construction:` instead.
const NARRATION_MARKER_SUMMARY: &str = "past-behavior narration marker: ";
/// Words that cannot head a `marker + participle + noun` modifier; after
/// them the participle stays predicate, as in `now returned to`.
const NON_MODIFIER_HEADS: &[&str] = &[
    "a", "an", "the", "to", "by", "in", "on", "at", "from", "with", "when", "while", "if", "and",
    "or", "but", "than", "that", "which", "where", "who", "until", "before", "after", "since",
    "as", "for", "per", "via", "into", "within", "without", "so", "yet", "nor", "because", "once",
    "also", "then", "of", "only",
];
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
/// Common state descriptions are ambiguous without an explicit agent.
const STATE_PARTICIPLES: &[&str] = &[
    "aligned",
    "allowed",
    "associated",
    "based",
    "bounded",
    "cached",
    "closed",
    "complicated",
    "concerned",
    "configured",
    "connected",
    "disabled",
    "disconnected",
    "enabled",
    "experienced",
    "fixed",
    "guaranteed",
    "installed",
    "intended",
    "inverted",
    "limited",
    "linked",
    "located",
    "locked",
    "mapped",
    "needed",
    "optimized",
    "recommended",
    "selected",
    "sorted",
    "supported",
    "unaffected",
    "unchanged",
    "uncompressed",
    "undefined",
    "unhandled",
    "uninitialized",
    "unsigned",
    "unsorted",
    "untested",
    "untouched",
    "unused",
    "visited",
    "zeroed",
];

/// One narration marker: its token sequence and display form.
struct NarrationMarker {
    tokens: &'static [&'static str],
    display: &'static str,
}

/// TEXT007 diagnostics for `doc`: at most one Hint per measured line,
/// in source order.
///
/// Finding classes share the code:
///
/// - Passive: a be-verb followed by a past participle, except accepted
///   adjectival participles.
/// - Narration marker: a phrase in [`NARRATION_MARKERS`], a word in
///   [`SINGLE_WORD_NARRATION_MARKERS`], a change noun with a change verb,
///   or clause-initial `before,`.
///
/// A line matching both classes reports the passive class only.
/// Code spans and link targets are opaque; punctuation interrupts phrases.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut code_delimiter = 0;
    let mut previous_number = 0;
    for line in &doc.lines {
        if line.number != previous_number + 1 || line.in_code_block || line.text.trim().is_empty() {
            code_delimiter = 0;
        }
        previous_number = line.number;

        if line.in_code_block {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.is_empty() || trimmed.starts_with('|') || is_link_reference_definition(trimmed) {
            continue;
        }
        if let Some(summary) = find_offense(trimmed, &mut code_delimiter) {
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

/// One TEXT007 Hint; `summary` names the finding class and trigger.
fn diagnostic(line: &StrippedLine, summary: &str) -> Diagnostic {
    let bullets = [
        "Treat this as a heuristic suggestion; preserve valid state descriptions and runtime history."
            .to_string(),
        "State only current behavior in active, present-tense language.".to_string(),
        "Remove change history, old/new comparisons, and time labels such as `now` or `currently`."
             .to_string(),
        "Delete implementation-history-only sentences; do not invent replacement behavior."
            .to_string(),
        "Check the implementation before rewriting; preserve exact conditions, guarantees, and limitations."
            .to_string(),
        "Keep implementation history out of comments and API docs, including internals, tests, and helpers. \
         Use release or migration notes only for a genuine public-API compatibility concern."
            .to_string(),
    ];

    Diagnostic {
        severity: Severity::Hint,
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
fn find_offense(line: &str, code_delimiter: &mut usize) -> Option<String> {
    let words = prose::words(line, code_delimiter);

    if let Some((be, participle)) = find_passive(line, &words) {
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

/// The first narration marker in the line, if any.
///
/// Phrase markers take precedence over single words. Context checks skip
/// common runtime descriptions; clause-initial `before,` still names history.
fn find_narration_marker(line: &str, words: &[prose::Word<'_>]) -> Option<&'static str> {
    for marker in NARRATION_MARKERS {
        if words
            .windows(marker.tokens.len())
            .enumerate()
            .any(|(index, w)| {
                prose::contiguous(line, w)
                    && w.iter()
                        .zip(marker.tokens)
                        .all(|(a, b)| a.text.eq_ignore_ascii_case(b))
                    && match marker.display {
                        "no longer" => !words
                            .get(index + w.len())
                            .is_some_and(|next| next.text.eq_ignore_ascii_case("than")),
                        "in the past" => {
                            is_clause_start(&line[..w[0].offset])
                                && w.last().is_some_and(|last| {
                                    line[last.offset + last.text.len()..].starts_with(',')
                                })
                        }
                        _ => true,
                    }
            })
        {
            return Some(marker.display);
        }
    }

    // Change nouns also name runtime data; require an implementation-change verb.
    for phrase in words.windows(3) {
        if phrase[0].text.eq_ignore_ascii_case("this")
            && prose::contiguous(line, phrase)
            && matches_any(
                phrase[2].text,
                &["adds", "fixes", "removes", "introduces", "rejects"],
            )
        {
            for (noun, marker) in [
                ("change", "this change"),
                ("patch", "this patch"),
                ("commit", "this commit"),
                ("update", "this update"),
                ("fix", "this fix"),
            ] {
                if phrase[1].text.eq_ignore_ascii_case(noun) {
                    return Some(marker);
                }
            }
        }
    }

    for marker in SINGLE_WORD_NARRATION_MARKERS {
        for (index, word) in words.iter().enumerate() {
            if !word.text.eq_ignore_ascii_case(marker) {
                continue;
            }

            // The hyphenated cache policy is a technical term, not history.
            if word.text.eq_ignore_ascii_case("recently")
                && index > 0
                && words.get(index + 1).is_some_and(|next| {
                    let previous = &words[index - 1];
                    previous.text.eq_ignore_ascii_case("least")
                        && next.text.eq_ignore_ascii_case("used")
                        && &line[previous.offset + previous.text.len()..word.offset] == "-"
                        && &line[word.offset + word.text.len()..next.offset] == "-"
                })
            {
                continue;
            }

            // Present-time markers keep attributive and intent contexts that
            // history markers such as `previously` still report.
            let present_time = matches_any(marker, &["now", "currently", "recently"]);

            // A modifier of runtime data is not implementation history.
            let modifies_state = words.get(index + 1).is_some_and(|next| {
                prose::contiguous(line, &words[index..=index + 1])
                    && (is_participle(next.text)
                        || matches_any(next.text, ADJECTIVAL_PARTICIPLES)
                        || matches_any(next.text, &["active", "available", "running", "known"]))
            });
            let follows_determiner = index > 0
                && prose::contiguous(line, &words[index - 1..=index])
                && matches_any(
                    words[index - 1].text,
                    &[
                        "a", "an", "the", "any", "all", "each", "least", "most", "no", "certain",
                    ],
                );

            // `now removed features` and `very recently loaded data` modify
            // a following noun rather than narrating a change.
            let modifies_noun = present_time
                && modifies_state
                && words.get(index + 2).is_some_and(|head| {
                    prose::contiguous(line, &words[index..=index + 2])
                        && !matches_any(head.text, NON_MODIFIER_HEADS)
                        && !ends_with_ci(head.text, "ly")
                });

            // `currently` also announces runtime states such as
            // `currently active`.
            let describes_runtime = word.text.eq_ignore_ascii_case("currently")
                && words.get(index + 1).is_some_and(|next| {
                    prose::contiguous(line, &words[index..=index + 1])
                        && matches_any(
                            next.text,
                            &[
                                "active",
                                "available",
                                "unavailable",
                                "running",
                                "supported",
                                "unused",
                                "interested",
                            ],
                        )
                });

            // `for now`, `now to <verb>`, and `now need` state present intent.
            let present_intent = word.text.eq_ignore_ascii_case("now")
                && ((index > 0
                    && words[index - 1].text.eq_ignore_ascii_case("for")
                    && prose::contiguous(line, &words[index - 1..=index]))
                    || words.get(index + 1).is_some_and(|next| {
                        prose::contiguous(line, &words[index..=index + 1])
                            && matches_any(next.text, &["to", "need", "needs"])
                    }));

            // A bare value after `currently`, as in `currently 0.1.157`,
            // reports current state rather than narrating a change.
            let announces_value = word.text.eq_ignore_ascii_case("currently")
                && words.get(index + 1).is_some_and(|next| {
                    prose::contiguous(line, &words[index..=index + 1])
                        && next.text.starts_with(|c: char| c.is_ascii_digit())
                });

            if !(modifies_state && follows_determiner
                || modifies_noun
                || describes_runtime
                || present_intent
                || announces_value)
            {
                return Some(marker);
            }
        }
    }

    words
        .iter()
        .any(|word| {
            word.text.eq_ignore_ascii_case("before")
                && line[word.offset + word.text.len()..].starts_with(',')
                && is_clause_start(&line[..word.offset])
        })
        .then_some("before,")
}

/// A be-verb immediately followed by a past participle, if any.
///
/// Adjectival participles such as `required` and `deprecated` are
/// accepted and never fire.
fn find_passive<'a>(line: &str, words: &[prose::Word<'a>]) -> Option<(&'a str, &'a str)> {
    words.windows(2).enumerate().find_map(|(index, pair)| {
        let be = pair[0].text;
        let participle = pair[1].text;
        if !prose::contiguous(line, pair)
            || !matches_any(be, BE_VERBS)
            || !is_participle(participle)
            || (participle.eq_ignore_ascii_case("left")
                && line[pair[1].offset + participle.len()..].starts_with("-to-right"))
        {
            return None;
        }

        let has_agent = words.get(index + 2).is_some_and(|next| {
            next.text.eq_ignore_ascii_case("by")
                && prose::contiguous(line, &words[index + 1..=index + 2])
        });

        // These complements form relational or modal idioms, not action claims.
        let state_idiom = words.get(index + 2).is_some_and(|next| {
            prose::contiguous(line, &words[index + 1..=index + 2])
                && next.text.eq_ignore_ascii_case("to")
                && matches_any(participle, &["related", "supposed"])
        });

        // Match legal wording narrowly rather than exempting distribution actions.
        let licensing_phrase = [
            &["licensed", "under"][..],
            &["distributed", "in", "the", "hope", "that"],
            &["permitted", "provided"],
        ]
        .iter()
        .any(|phrase| {
            words
                .get(index + 1..index + 1 + phrase.len())
                .is_some_and(|tail| {
                    prose::contiguous(line, tail)
                        && tail
                            .iter()
                            .zip(*phrase)
                            .all(|(word, expected)| word.text.eq_ignore_ascii_case(expected))
                })
        });

        // A hyphen directly before `be` marks a compound such as
        // `soon-to-be removed`; a real auxiliary never glues to a hyphen.
        let compound_be = be.eq_ignore_ascii_case("be") && line[..pair[0].offset].ends_with('-');

        // `provided "as is"` closes license grants; quotes may join the idiom.
        let provided_as_is = participle.eq_ignore_ascii_case("provided") && {
            let tail = line[pair[1].offset + participle.len()..]
                .trim_start_matches([' ', '"', '\'', '(', '[']);
            let bytes = tail.as_bytes();
            let head = &bytes[..bytes.len().min(5)];
            (head.eq_ignore_ascii_case(b"as is") || head.eq_ignore_ascii_case(b"as-is"))
                && bytes
                    .get(5)
                    .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
        };

        (!licensing_phrase
            && !state_idiom
            && !compound_be
            && !provided_as_is
            && (!matches_any(participle, STATE_PARTICIPLES) || has_agent))
            .then_some((be, participle))
    })
}

/// Whether the text before one `before,` occurrence ends a clause, so
/// the marker itself starts a new clause.
fn is_clause_start(prefix: &str) -> bool {
    let trimmed = prefix
        .trim()
        .trim_start_matches(['-', '*', '+', '>', '#'])
        .trim();
    trimmed.is_empty()
        || trimmed
            .chars()
            .last()
            .is_some_and(|c| matches!(c, '.' | '!' | '?' | ';' | ':'))
}

/// Whether `word` resembles a participle rather than a known ambiguous word.
fn is_participle(word: &str) -> bool {
    if matches_any(word, ADJECTIVAL_PARTICIPLES) {
        return false;
    }
    (word.bytes().all(|byte| byte.is_ascii_alphabetic())
        && ends_with_ci(word, "ed")
        && !matches_any(word, ED_NON_PARTICIPLES))
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

    // Each unambiguous be-verb plus participle produces one hint.
    #[test]
    fn hints_should_identify_passive_pairs_when_unambiguous() {
        for (source, pair) in [
            ("Errors are returned by the scanner.", "are returned"),
            ("The value was parsed by the loader.", "was parsed"),
            ("Links will be rewritten.", "be rewritten"),
            ("The file is distributed to clients.", "is distributed"),
            ("The module is initialized by the build.", "is initialized"),
            ("The bytes are signed with a certificate.", "are signed"),
        ] {
            let found = one_line(source);
            assert_eq!(found.len(), 1, "{source:?}");
            assert_eq!(found[0].severity, Severity::Hint);
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
        assert!(one_line("Fix records are unnamed, so they omit the item name.").is_empty());
        assert!(one_line("This is outdated.").is_empty());
    }

    #[test]
    fn hints_should_allow_ambiguous_states_when_no_agent_follows() {
        for participle in STATE_PARTICIPLES {
            let state = format!("The values are {participle}.");
            let action = format!("The values are {participle} by the scanner.");

            let state_findings = one_line(&state);
            let action_findings = one_line(&action);

            assert!(state_findings.is_empty(), "{state}");
            assert_eq!(action_findings.len(), 1, "{action}");
            assert!(
                action_findings[0]
                    .message
                    .starts_with("passive construction:")
            );
        }
    }

    // Bare `en` words are state adjectives or nouns, never passives.
    #[test]
    fn text_checks_silent_on_non_participle_en_words() {
        assert!(one_line("The door is open.").is_empty());
        assert!(one_line("The count is ten.").is_empty());
        assert!(one_line("The word is often misspelled.").is_empty());
    }

    // Controlled `en` participles after a be-verb still produce hints.
    #[test]
    fn hints_should_identify_passive_voice_when_using_en_participles() {
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

    // Each single-word and phrase marker produces one hint naming the marker.
    #[test]
    fn text_checks_should_name_marker_when_prose_narrates_behavior() {
        for (source, marker) in [
            ("This no longer panics.", "no longer"),
            ("The old path previously ran here.", "previously"),
            ("The cache is now bounded.", "now"),
            (
                "Prior to this change, input could panic.",
                "prior to this change",
            ),
            ("Before this change, the cache grew.", "before this change"),
            (
                "After this change, names must be nonempty.",
                "after this change",
            ),
            (
                "With this change, callers receive an error.",
                "with this change",
            ),
            ("This change adds validation.", "this change"),
            ("This patch fixes error handling.", "this patch"),
            ("This commit removes the fallback.", "this commit"),
            ("This update introduces validation.", "this update"),
            (
                "The previous implementation copied input.",
                "previous implementation",
            ),
            (
                "Unlike the old implementation, this borrows input.",
                "old implementation",
            ),
            ("Earlier versions accepted empty names.", "earlier versions"),
            ("Previous versions ignored errors.", "previous versions"),
            ("THIS PATCH fixes error handling.", "this patch"),
            (
                "In earlier releases, empty input could panic.",
                "in earlier releases",
            ),
            (
                "In previous releases, empty input could panic.",
                "in previous releases",
            ),
            (
                "In prior releases, empty input could panic.",
                "in prior releases",
            ),
            (
                "The earlier implementation copied input.",
                "earlier implementation",
            ),
            (
                "The prior implementation copied input.",
                "prior implementation",
            ),
            (
                "The original implementation copied input.",
                "original implementation",
            ),
            ("This preserves the earlier behavior.", "earlier behavior"),
            ("This preserves the previous behavior.", "previous behavior"),
            ("This preserves the prior behavior.", "prior behavior"),
            ("This preserves the old behavior.", "old behavior"),
            ("This preserves the earlier behaviour.", "earlier behaviour"),
            (
                "This preserves the previous behaviour.",
                "previous behaviour",
            ),
            ("This preserves the prior behaviour.", "prior behaviour"),
            ("This preserves the old behaviour.", "old behaviour"),
            (
                "Before this fix, empty input could panic.",
                "before this fix",
            ),
            (
                "After this fix, empty input returns an error.",
                "after this fix",
            ),
            (
                "With this fix, empty input returns an error.",
                "with this fix",
            ),
            ("This fix rejects empty input.", "this fix"),
            (
                "As of this release, names must be nonempty.",
                "as of this release",
            ),
            (
                "As of this version, names must be nonempty.",
                "as of this version",
            ),
            (
                "BEFORE THIS FIX, empty input could panic.",
                "before this fix",
            ),
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
            assert_eq!(found[0].severity, Severity::Hint);
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
            "Retry until the queue is empty.",
            "Return an error if the key already exists.",
            "Return an error instead of panicking.",
            "Accept no more than ten entries.",
            "Reject any longer names.",
            "This commitment lasts until shutdown.",
            "This updater validates names.",
            "This changes the buffer size.",
            "This patchwork contains several regions.",
            "This fixture contains empty names.",
            "Read the snapshot as of this timestamp.",
            "The flag tracks whether the state changed from idle to active.",
            "The flag tracks whether the state changed to active.",
            "This operation replaces the old value.",
            "This operation replaces the previous entry.",
            "Accept aliases for backward compatibility.",
            "Accept aliases for backwards compatibility.",
            "The status indicator is red.",
            "The next field is seed.",
            "The client is disconnected.",
            "The values are sorted.",
            "The flag is disabled.",
            "Keep the values as they are. Sorted input avoids allocation.",
            "Call `Clock::now()` to read the clock.",
            "Return `was_cached` for cache hits.",
            "Read the [clock reference](https://example.test/now).",
            "Accept keys no longer than ten bytes.",
            "Reject timestamps in the past.",
            "Evict the least recently used entry.",
            "Return the currently selected item.",
            "The currently active loadout cannot change.",
            "Lists the currently available library features.",
            "Not currently available in the C# version.",
            "Update the application you are currently running.",
            "Lists currently supported architectures and their features.",
            "Other bits are currently unused.",
            "English is left-to-right and top-to-bottom.",
            "This holds for all currently known versions.",
            "Evict entries from the least-recently-used cache.",
            "Use a typical setup for a certain recently released game.",
            "I am currently interested in crash reports.",
            "The user experience is unchanged.",
            "Write operations are unaffected.",
            "These items are unsorted and do not have a sort index.",
            "Forking and merging is complicated.",
            "Some users are experienced.",
            "The columns are linked together.",
            "The buffer is locked.",
            "The nested model key is undefined.",
            "The rejected promise is unhandled.",
            "The input is untouched.",
            "The pointers are aligned.",
            "The offset is unsigned 32-bit.",
            "The variable is uninitialized.",
            // State participles sampled as false positives in local projects.
            "The library is based on libusb.",
            "The cell is selected.",
            "The gh CLI is installed.",
            "The flag value is inverted.",
            "The policy is configured per profile.",
            "The index the key is mapped to stays stable.",
            "Delivery is guaranteed.",
            "This node has been visited.",
            "The new memory is zeroed.",
            "Fast variants are optimized for lower latency.",
            "The mods are associated with the game.",
            "As far as loading mods is concerned, this works.",
            // License wording embedded in ordinary source files.
            "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND.",
            "Redistribution and use are permitted provided that the notice stays.",
            // Changelog templates and present-time marker uses.
            "`Deprecated` marks soon-to-be removed features.",
            "`Removed` marks now removed features.",
            "Prefetch very recently loaded data.",
            "For now, we're only testing estimated sizes.",
            "Reload now to clear the gestures.",
            "We now need to iterate the table.",
            "Update the version (currently 0.1.157).",
            "Return true if the previous request was successful.",
            "Apply this patch to the input buffer.",
            "Track the bytes used to compute the checksum.",
            // Ambiguous markers are deliberately omitted even in historical prose.
            "This flag used to default on.",
            "The value was large.",
        ] {
            let found = one_line(source);

            assert!(found.is_empty(), "{source:?}");
        }
    }

    // Code and links interrupt phrases without hiding the prose that follows.
    #[test]
    fn hints_should_ignore_nonprose_and_boundaries_when_scanning_comments_and_markdown() {
        for (source, expected) in [
            ("Keep values as they are; sorted input helps.", false),
            ("Keep values as they are! Sorted input helps.", false),
            ("The value is `a field` returned to the caller.", false),
            ("This is_red and was_cached.", false),
            ("The key is red2.", false),
            ("The key is type_returned.", false),
            ("No. Longer names need validation.", false),
            ("Read ``Clock::`now`()`` to get the clock.", false),
            ("Read [docs](https://example.test/(now)).", false),
            ("Read [docs](https://example.test/now\\)).", false),
            ("Read [docs][now].", false),
            ("Read <https://example.test/now>.", false),
            ("Read https://example.test/now for details.", false),
            ("Use <span id=\"now\">the clock</span>.", false),
            ("Use `Before, the cache grew.` as input.", false),
            (
                "Use \\` as a delimiter. Errors are returned by the scanner.",
                true,
            ),
            ("Use ``now ` currently`` as input.", false),
            ("Read [docs](unterminated now", false),
            ("Use <span>Errors are returned by the scanner.</span>", true),
            ("Use `now` here. Errors are returned by the scanner.", true),
            (
                "Read [docs](https://example.test/now). This no longer panics.",
                true,
            ),
            ("The values are sorted by the scanner.", true),
            ("Errors ARE RETURNED by the scanner.", true),
            ("Before, the cache grew.", true),
            ("- Before, the cache grew.", true),
            ("# In the past, the cache grew.", true),
            ("`first\nClock::now()\nlast`", false),
            ("`first\nlast` Errors are returned by the scanner.", true),
            ("Errors are returned. `first\nClock::now()\nlast`", true),
            ("`unclosed\n\nErrors are returned by the scanner.", true),
            (
                "`unclosed\n```rust\nClock::now();\n```\nErrors are returned.",
                true,
            ),
            ("Errors are\nreturned by the scanner.", false),
            ("This program is licensed under the MIT license.", false),
            (
                "This program IS DISTRIBUTED IN THE HOPE THAT it will be useful.",
                false,
            ),
            ("The file is distributed in the output folder.", true),
            ("The file is left to the caller.", true),
            ("The collection is supposed to be read-only.", false),
            ("The work is related to data management.", false),
            ("The story is related by the narrator.", true),
            ("The work is related. To continue, read the guide.", true),
            ("The work is related to storage. Errors are returned.", true),
            (
                "The program is licensed. Under load, it returns errors.",
                true,
            ),
            ("This is licensed under MIT. Errors are returned.", true),
            // License grants with an agent or without the closing idiom still fire.
            ("Credits are provided by the vendor.", true),
            ("The notice is provided in each copy.", true),
            // Compound auxiliaries glue to a hyphen; plain infinitives do not.
            ("The feature will soon be removed.", true),
            // Predicate participles after a present-time marker still fire.
            ("The values are now sorted alphabetically.", true),
            // Attributive history modifiers stay reportable.
            ("Previously released versions ignored errors.", true),
            ("The currently. Active parser returns errors.", true),
            ("The currently `active` parser returns errors.", true),
            ("The recently-rewritten parser returns errors.", true),
            ("Use the least-recently-tested parser.", true),
            ("Use the least. Recently-used cache.", true),
            (
                "Use the least-recently-used cache. Previously, this grew.",
                true,
            ),
        ] {
            for ext in ["md", "rs"] {
                let input = if ext == "rs" {
                    source.lines().map(|line| format!("/// {line}\n")).collect()
                } else {
                    source.to_string()
                };

                let diags = run_text_checks(&input, ext);
                let found = codes(&diags, CODE_PASSIVE_NARRATION);

                assert_eq!(found.len(), usize::from(expected), "{ext}: {source:?}");
                assert!(found.iter().all(|diag| diag.severity == Severity::Hint));
            }
        }
    }

    // Source-code gaps end open inline code spans between comment regions.
    #[test]
    fn hints_should_resume_after_code_when_comment_regions_are_separate() {
        let source = "/// `unclosed\nfn boundary() {}\n/// Errors are returned by the scanner.\n";

        let diags = run_text_checks(source, "rs");
        let found = codes(&diags, CODE_PASSIVE_NARRATION);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
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
            "\n  - Treat this as a heuristic suggestion; preserve valid state descriptions and runtime history.",
            "\n  - State only current behavior in active, present-tense language.",
            "\n  - Remove change history, old/new comparisons, ",
            "and time labels such as `now` or `currently`.",
            "\n  - Delete implementation-history-only sentences; do not invent replacement behavior.",
            "\n  - Check the implementation before rewriting; ",
            "preserve exact conditions, guarantees, and limitations.",
            "\n  - Keep implementation history out of comments and API docs, ",
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
