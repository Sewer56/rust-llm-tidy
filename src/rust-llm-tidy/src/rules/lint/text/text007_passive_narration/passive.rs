//! Passive-voice tables and matching for the be-verb plus participle class.

use super::prose;

/// Participles accepted as adjectives after a be-verb; `is required` and
/// `is deprecated` describe state, not voice.
pub(super) const ADJECTIVAL_PARTICIPLES: &[&str] =
    &["required", "deprecated", "unnamed", "outdated"];
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

/// A be-verb immediately followed by a past participle, if any.
///
/// Adjectival participles such as `required` and `deprecated` are
/// accepted and never fire.
pub(super) fn find_passive<'a>(
    line: &str,
    words: &[prose::Word<'a>],
) -> Option<(&'a str, &'a str)> {
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

/// Whether `word` resembles a participle rather than a known ambiguous word.
pub(super) fn is_participle(word: &str) -> bool {
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
pub(super) fn ends_with_ci(word: &str, suffix: &str) -> bool {
    word.is_ascii()
        && word.len() > suffix.len()
        && word[word.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Whether `word` equals any entry of `words`, ASCII-case-insensitively.
pub(super) fn matches_any(word: &str, words: &[&str]) -> bool {
    words.iter().any(|known| word.eq_ignore_ascii_case(known))
}

#[cfg(test)]
mod tests {
    use super::super::one_line;
    use super::*;
    use crate::reporting::diagnostic::Severity;

    // ── TEXT007: passive constructions ──

    // Each unambiguous be-verb plus participle produces one reminder.
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
            assert_eq!(found[0].severity, Severity::Reminder);
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
}
