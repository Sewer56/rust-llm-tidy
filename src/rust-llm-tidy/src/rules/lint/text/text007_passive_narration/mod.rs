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
//!
//! # Layout
//!
//! - `narration` - narration-marker tables and phrase matching.
//! - `passive` - be-verb and participle tables with passive matching.
//! - `prose` - word collection that skips code and link targets.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
#[cfg(test)]
use crate::rules::lint::run_text_checks;
#[cfg(test)]
use crate::rules::lint::tests::codes;
use crate::rules::registry::CODE_PASSIVE_NARRATION;
use crate::text::measurement::is_link_reference_definition;
use crate::text::measurement::{Document, StrippedLine};
use narration::find_narration_marker;
use passive::find_passive;

mod narration;
mod passive;
mod prose;

/// Summary prefix carried by every narration-marker diagnostic; the
/// passive class opens with `passive construction:` instead.
const NARRATION_MARKER_SUMMARY: &str = "past-behavior narration marker: ";

/// TEXT007 diagnostics for `doc`: at most one Hint per measured line,
/// in source order.
///
/// Finding classes share the code:
///
/// - Passive: a be-verb followed by a past participle, except accepted
///   adjectival participles.
/// - Narration marker: a phrase in `NARRATION_MARKERS`, a word in
///   `SINGLE_WORD_NARRATION_MARKERS`, a change noun with a change verb,
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

/// TEXT007 findings for one measured markdown prose line.
#[cfg(test)]
pub(super) fn one_line(line: &str) -> Vec<crate::reporting::Diagnostic> {
    let diags = run_text_checks(&format!("{line}\n"), "md");
    codes(&diags, CODE_PASSIVE_NARRATION)
        .into_iter()
        .cloned()
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use indoc::formatdoc;

    // ── TEXT007: one diagnostic per line and message shape ──

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
