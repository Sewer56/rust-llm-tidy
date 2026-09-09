//! Narration-marker tables and matching for the history-wording class.

use super::passive::{ADJECTIVAL_PARTICIPLES, ends_with_ci, is_participle, matches_any};
use super::prose;

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

/// One narration marker: its token sequence and display form.
struct NarrationMarker {
    tokens: &'static [&'static str],
    display: &'static str,
}

/// The first narration marker in the line, if any.
///
/// Four ordered passes run, first match wins: phrase markers, change
/// nouns, single words, then clause-initial `before,`. Context checks
/// skip common runtime descriptions; clause-initial `before,` still
/// names history.
pub(super) fn find_narration_marker(line: &str, words: &[prose::Word<'_>]) -> Option<&'static str> {
    phrase_marker(line, words)
        .or_else(|| change_noun_marker(line, words))
        .or_else(|| single_word_marker(line, words))
        .or_else(|| before_comma_marker(line, words))
}

/// The clause-initial `before,` marker in the line, if any; temporal
/// `before` never matches.
fn before_comma_marker(line: &str, words: &[prose::Word<'_>]) -> Option<&'static str> {
    words
        .iter()
        .any(|word| {
            word.text.eq_ignore_ascii_case("before")
                && line[word.offset + word.text.len()..].starts_with(',')
                && is_clause_start(&line[..word.offset])
        })
        .then_some("before,")
}

/// The first `this <noun> <verb>` change marker in the line, if any.
fn change_noun_marker(line: &str, words: &[prose::Word<'_>]) -> Option<&'static str> {
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

    None
}

/// The first `NARRATION_MARKERS` phrase in the line, if any; `no longer
/// than` comparisons and non-clause-initial `in the past` stay silent.
fn phrase_marker(line: &str, words: &[prose::Word<'_>]) -> Option<&'static str> {
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

    None
}

/// The first `SINGLE_WORD_NARRATION_MARKERS` word in the line, if any;
/// runtime-context exceptions stay silent.
fn single_word_marker(line: &str, words: &[prose::Word<'_>]) -> Option<&'static str> {
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

    None
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

#[cfg(test)]
mod tests {
    use super::super::is_narration_marker;
    use super::super::one_line;
    use crate::reporting::diagnostic::Severity;
    use rstest::rstest;

    // ── TEXT007: narration markers ──

    // Each single-word and phrase marker produces one hint naming the marker.
    #[rstest]
    #[case::no_longer("This no longer panics.", "no longer")]
    #[case::previously("The old path previously ran here.", "previously")]
    #[case::now("The cache is now bounded.", "now")]
    #[case::prior_to_this_change(
        "Prior to this change, input could panic.",
        "prior to this change"
    )]
    #[case::before_this_change("Before this change, the cache grew.", "before this change")]
    #[case::after_this_change("After this change, names must be nonempty.", "after this change")]
    #[case::with_this_change("With this change, callers receive an error.", "with this change")]
    #[case::this_change("This change adds validation.", "this change")]
    #[case::this_patch("This patch fixes error handling.", "this patch")]
    #[case::this_commit("This commit removes the fallback.", "this commit")]
    #[case::this_update("This update introduces validation.", "this update")]
    #[case::previous_implementation(
        "The previous implementation copied input.",
        "previous implementation"
    )]
    #[case::old_implementation(
        "Unlike the old implementation, this borrows input.",
        "old implementation"
    )]
    #[case::earlier_versions("Earlier versions accepted empty names.", "earlier versions")]
    #[case::previous_versions("Previous versions ignored errors.", "previous versions")]
    #[case::this_patch_uppercase("THIS PATCH fixes error handling.", "this patch")]
    #[case::in_earlier_releases(
        "In earlier releases, empty input could panic.",
        "in earlier releases"
    )]
    #[case::in_previous_releases(
        "In previous releases, empty input could panic.",
        "in previous releases"
    )]
    #[case::in_prior_releases("In prior releases, empty input could panic.", "in prior releases")]
    #[case::earlier_implementation(
        "The earlier implementation copied input.",
        "earlier implementation"
    )]
    #[case::prior_implementation("The prior implementation copied input.", "prior implementation")]
    #[case::original_implementation(
        "The original implementation copied input.",
        "original implementation"
    )]
    #[case::earlier_behavior("This preserves the earlier behavior.", "earlier behavior")]
    #[case::previous_behavior("This preserves the previous behavior.", "previous behavior")]
    #[case::prior_behavior("This preserves the prior behavior.", "prior behavior")]
    #[case::old_behavior("This preserves the old behavior.", "old behavior")]
    #[case::earlier_behaviour("This preserves the earlier behaviour.", "earlier behaviour")]
    #[case::previous_behaviour("This preserves the previous behaviour.", "previous behaviour")]
    #[case::prior_behaviour("This preserves the prior behaviour.", "prior behaviour")]
    #[case::old_behaviour("This preserves the old behaviour.", "old behaviour")]
    #[case::before_this_fix("Before this fix, empty input could panic.", "before this fix")]
    #[case::after_this_fix("After this fix, empty input returns an error.", "after this fix")]
    #[case::with_this_fix("With this fix, empty input returns an error.", "with this fix")]
    #[case::this_fix("This fix rejects empty input.", "this fix")]
    #[case::as_of_this_release("As of this release, names must be nonempty.", "as of this release")]
    #[case::as_of_this_version("As of this version, names must be nonempty.", "as of this version")]
    #[case::before_this_fix_uppercase(
        "BEFORE THIS FIX, empty input could panic.",
        "before this fix"
    )]
    #[case::in_the_past("In the past, the cache grew without a bound.", "in the past")]
    #[case::formerly("The parser formerly accepted empty names.", "formerly")]
    #[case::historically("Historically, the parser accepted empty names.", "historically")]
    #[case::originally("The parser originally accepted empty names.", "originally")]
    #[case::recently("The parser recently gained a size limit.", "recently")]
    #[case::lately("Lately, the parser rejects empty names.", "lately")]
    #[case::currently("The parser currently rejects empty names.", "currently")]
    #[case::anymore("The parser does not accept empty names anymore.", "anymore")]
    #[case::formerly_uppercase("FORMERLY, the cache grew without a bound.", "formerly")]
    #[case::in_the_past_uppercase("IN THE PAST, the cache grew without a bound.", "in the past")]
    fn text_checks_should_name_marker_when_prose_narrates_behavior(
        #[case] source: &str,
        #[case] marker: &str,
    ) {
        let found = one_line(source);

        assert_eq!(found.len(), 1, "{source:?}");
        assert_eq!(found[0].severity, Severity::Reminder);
        assert!(is_narration_marker(&found[0]), "{source:?}");
        assert!(
            found[0]
                .message
                .starts_with(&format!("past-behavior narration marker: `{marker}`.")),
            "{}",
            found[0].message
        );
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
}
