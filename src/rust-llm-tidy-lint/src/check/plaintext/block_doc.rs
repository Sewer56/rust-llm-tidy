//! The block doc dialect: measuring [`DocRegion`]s whose lines carry
//! `/** ... */`-style block-comment content (Javadoc, JSDoc, Doxygen).
//!
//! The producer removes the `/*` and `*/` delimiters; this dialect then
//! strips each line's leading `*` continuation marker and exempts the
//! tag token (plus the name argument of name-taking tags like `@param`)
//! from measurement.
//!
//! The remaining prose feeds the shared markdown classifier, so blank
//! lines split paragraphs and fenced or indented example blocks are
//! exempt.

use super::region::DocRegion;
use super::{Document, PendingParagraph, flush, measure_prose_line};

/// Tag words whose first argument token is a name (`@param name`, `@throws
/// IOException`): the tag and that name are exempt together.
const NAME_TAGS: &[&str] = &[
    "param",
    "arg",
    "argument",
    "property",
    "prop",
    "throws",
    "exception",
];

/// Measures one block doc region into `doc`'s lines and paragraphs.
///
/// Paragraph and fence state flow exactly as for the markdown dialect, so
/// prose never outlives the region: the measuring core flushes at every
/// region boundary.
pub(super) fn measure_region(
    region: DocRegion,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
    in_fence: &mut bool,
) {
    for line in region.lines {
        let mut text = strip_continuation(line.text);
        let prefix = tag_prefix_len(&text);
        if prefix > 0 {
            // A block tag starts its own block: the description never
            // joins it, and consecutive tags never join each other.
            flush(pending, doc);
            // Drain the exempt prefix in place: the owned buffer moves
            // on to the measured line without a second allocation.
            text.replace_range(..prefix, "");
            measure_prose_line(text, line.number, false, doc, pending, in_fence);
        } else {
            let indented = prose_is_indented(&text);
            measure_prose_line(text, line.number, indented, doc, pending, in_fence);
        }
    }
}

/// Whether the measured prose counts as indented code: a tab or 4-space
/// lead after the `*` continuation marker.
fn prose_is_indented(text: &str) -> bool {
    text.starts_with('\t') || text.starts_with("    ")
}

/// Strips the line's `*` continuation marker in place: a maximal leading
/// `*` run vanishes when only whitespace (at most one space of it) or
/// nothing follows, and a lone `*`-led word keeps one marker so `* - item`
/// stays a recognizable bullet.
///
/// ```text
/// "* Parses."  -> "Parses."
/// "**"         -> ""
/// "* * item"   -> "* item"
/// "prose"      -> "prose"
/// ```
fn strip_continuation(mut text: String) -> String {
    let run = text.chars().take_while(|&c| c == '*').count();
    if run == 0 {
        return text;
    }
    let rest = &text[run..];
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
        let keep_space = usize::from(rest.starts_with(' '));
        text.replace_range(..run + keep_space, "");
    } else {
        text.replace_range(..1, "");
    }
    text
}

/// The byte length of a tag line's exempt prefix: the tag token, the
/// name argument of name-taking tags, an optional JSDoc `{type}` group,
/// and the separating whitespace. Zero when the line does not start
/// with `@` plus a tag word.
///
/// ```text
/// "@param name parses the input"   -> exempt "@param name"
/// "@param {string} name the input" -> exempt "@param {string} name"
/// "@return Returns the value"      -> exempt "@return"
/// "@deprecated"                    -> exempt "@deprecated"
/// ```
fn tag_prefix_len(text: &str) -> usize {
    let Some(after_at) = text.strip_prefix('@') else {
        return 0;
    };
    let word_end = after_at
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(after_at.len());
    if word_end == 0 {
        return 0;
    }
    let word = &after_at[..word_end];
    let mut rest = after_at[word_end..].trim_start();
    if !NAME_TAGS.contains(&word) {
        return text.len() - rest.len();
    }
    // JSDoc types: `@param {string} name prose` exempts the brace group.
    if let Some(after_brace) = rest.strip_prefix('{')
        && let Some(close) = after_brace.find('}')
    {
        rest = after_brace[close + 1..].trim_start();
    }
    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let prose = rest[name_end..].trim_start();
    text.len() - prose.len()
}

#[cfg(test)]
mod tests {
    use crate::check::CODE_LINE_LENGTH;
    use crate::check::CODE_PARAGRAPH_SIZE;
    use crate::check::plaintext::region::{Dialect, DocRegion, RegionLine};
    use crate::check::plaintext::run_region_checks;
    use crate::check::plaintext::tests::codes;
    use crate::diagnostic::Diagnostic;

    /// Builds one block-doc region from `(line number, text)` pairs.
    fn block_region(lines: &[(usize, &str)]) -> DocRegion {
        DocRegion {
            dialect: Dialect::BlockDoc,
            lines: lines
                .iter()
                .map(|(number, text)| RegionLine {
                    number: *number,
                    text: (*text).to_string(),
                    indented: false,
                })
                .collect(),
        }
    }

    /// Runs the text checks over one block-doc region.
    fn measure(lines: &[(usize, &str)]) -> Vec<Diagnostic> {
        run_region_checks(vec![block_region(lines)])
    }

    // ── `*` continuation stripping ──

    // Prose joins across `*`-continued lines into one paragraph measured
    // without the markers; DOC008 counts the stripped text only.
    #[test]
    fn star_continuations_strip_and_join_prose() {
        let filler = "word ".repeat(26);
        let line = filler.trim();
        let diags = measure(&[
            (2, "*"),
            (3, &format!("* {line}")),
            (4, &format!("* {line}")),
        ]);

        let paragraphs = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].line, 3, "the first prose line");

        let long = "x".repeat(81);
        let diags = measure(&[(7, &format!("* {long}"))]);
        let lines = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line, 7);
    }

    // Heavy `***` opener and closer lines carry no prose: they measure as
    // blanks instead of stray marker text.
    #[test]
    fn heavy_star_marker_lines_measure_blank() {
        let filler = "z".repeat(70);
        let diags = measure(&[
            (1, "**"),
            (2, &filler),
            (3, &filler),
            (4, &filler),
            (5, &filler),
            (6, "**"),
        ]);

        assert_eq!(codes(&diags, CODE_PARAGRAPH_SIZE).len(), 1);
        assert_eq!(codes(&diags, CODE_LINE_LENGTH).len(), 0);
    }

    // ── `@tag` lines ──

    // The tag token and the name argument are exempt: a tag line whose
    // prose is short stays quiet even when the raw line is over budget.
    #[test]
    fn name_tags_exempt_tag_and_name_tokens() {
        let name = "n".repeat(40);
        let quiet = "p".repeat(79);
        let control = "x".repeat(81);
        let diags = measure(&[(3, &format!("@param {name} {quiet}")), (4, &control)]);

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1, "the tag line stays quiet");
        assert_eq!(found[0].line, 4);
    }

    // JSDoc type groups vanish with the tag and name.
    #[test]
    fn jsdoc_type_groups_are_exempt() {
        let spec = "a very long type specification indeed".to_string();
        let quiet = "p".repeat(79);
        let control = "x".repeat(81);
        let diags = measure(&[(5, &format!("@param {{{spec}}} x {quiet}")), (6, &control)]);

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1, "the type-bearing tag line stays quiet");
        assert_eq!(found[0].line, 6);
    }

    // Tags without a name argument exempt only the tag token.
    #[test]
    fn non_name_tags_exempt_only_the_tag() {
        let long = "p".repeat(81);
        let diags = measure(&[(2, &format!("@return {long}"))]);

        assert_eq!(codes(&diags, CODE_LINE_LENGTH).len(), 1);
    }

    // Block tags never join prose or each other: two 150-char param
    // descriptions stay silent where a joined paragraph would overflow.
    #[test]
    fn tag_lines_start_their_own_paragraphs() {
        let desc = "d".repeat(150);
        let diags = measure(&[
            (1, "Describes the call."),
            (2, &format!("@param first {desc}")),
            (3, &format!("@param second {desc}")),
        ]);

        assert!(
            codes(&diags, CODE_PARAGRAPH_SIZE).is_empty(),
            "tag descriptions must not join"
        );
    }

    // A tag-only line measures as a blank: it splits the paragraph.
    #[test]
    fn tag_only_lines_split_paragraphs() {
        let filler = "z".repeat(120);
        let diags = measure(&[(1, &filler), (2, "@deprecated"), (3, &filler)]);

        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // ── Exempt example blocks ──

    // Fenced example blocks are exempt, delimiters included, and their
    // content never warns on length.
    #[test]
    fn fenced_examples_are_exempt() {
        let long = "c".repeat(90);
        let diags = measure(&[
            (1, "Prose."),
            (2, "```"),
            (3, &long),
            (4, "```"),
            (5, "More prose."),
        ]);

        assert!(diags.is_empty(), "fenced example stays fully quiet");
    }

    // A tab or 4-space lead after the `* ` separator is indented example
    // code.
    #[test]
    fn indented_examples_are_exempt() {
        let long = "c".repeat(90);
        let diags = measure(&[
            (1, "Prose."),
            (2, &format!("*     {long}")),
            (3, "More prose."),
        ]);

        assert!(diags.is_empty(), "indented example stays fully quiet");
    }
}
