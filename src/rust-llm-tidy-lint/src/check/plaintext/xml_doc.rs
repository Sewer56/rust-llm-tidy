//! The XML doc dialect: measuring [`DocRegion`]s whose lines carry XML
//! doc-comment markup (`<summary>`, `<param name="...">`, ...).
//!
//! Only the inner text of text nodes is measured: tags vanish, attribute
//! values (including `cref` and `name`) never count, and `<code>` and
//! `<example>` subtrees are exempt like code fences.
//!
//! A paragraph is a contiguous text run within one tag, so prose never
//! joins across a tag boundary; a whitespace-only text node splits like
//! a blank line.

use super::region::DocRegion;
use super::{Document, ParagraphKind, PendingParagraph, StrippedLine, flush};

/// Scanner state carried across a region's lines.
#[derive(Default)]
struct TagScan {
    /// Inside a `<...>` span that has not closed yet; a span may open on
    /// one line and close on a later one.
    in_tag: bool,
    /// The open quote inside the tag being scanned, if any; a quoted
    /// attribute value may contain `>`.
    quote: Option<char>,
    /// Nesting depth of `<code>` and `<example>` subtrees; text inside
    /// them is exempt from both checks.
    exempt: usize,
    /// Whether the open paragraph continues into the next text node:
    /// true only when a text node reached the end of its line without a
    /// tag boundary after it.
    run_open: bool,
}

/// Measures one XML doc region into `doc`'s lines and paragraphs.
///
/// Paragraph state flows through `pending` exactly as for the markdown
/// dialect, so paragraphs never outlive the region: the measuring core
/// flushes at every region boundary.
pub(super) fn measure_region(
    region: DocRegion,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
) {
    let mut scan = TagScan::default();
    for line in region.lines {
        scan_line(&line.text, line.number, &mut scan, doc, pending);
    }
}

/// Scans one line: folds its text nodes into the measured line text and
/// the open paragraph, and advances the tag state.
fn scan_line(
    text: &str,
    number: usize,
    scan: &mut TagScan,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
) {
    let mut measured = String::with_capacity(text.len());
    // Whether any non-whitespace text landed inside an exempt subtree.
    let mut saw_code_text = false;
    // Bounds within `text` of the current text node and tag fragment.
    let mut seg_start = 0;
    let mut frag_start = 0;

    for (idx, ch) in text.char_indices() {
        if scan.in_tag {
            match scan.quote {
                Some(q) if ch == q => scan.quote = None,
                Some(_) => {}
                None if ch == '"' || ch == '\'' => scan.quote = Some(ch),
                None if ch == '>' => {
                    close_tag(&text[frag_start..idx], scan);
                    flush(pending, doc);
                    scan.run_open = false;
                    scan.in_tag = false;
                    seg_start = idx + 1;
                }
                None => {}
            }
            continue;
        }
        if ch != '<' {
            continue;
        }
        let segment = &text[seg_start..idx];
        if scan.exempt > 0 {
            saw_code_text |= !segment.trim().is_empty();
        } else {
            measure_segment(segment, number, scan, pending, doc, &mut measured);
        }
        // A tag boundary is a paragraph boundary: prose never joins
        // across it.
        flush(pending, doc);
        scan.run_open = false;
        scan.in_tag = true;
        scan.quote = None;
        frag_start = idx + 1;
    }

    // The line's tail: a text node when the line ends outside a tag, or
    // markup swallowed by an open tag that spans lines.
    let tail = if scan.in_tag { "" } else { &text[seg_start..] };
    if scan.exempt > 0 {
        saw_code_text |= !tail.trim().is_empty();
    } else {
        measure_segment(tail, number, scan, pending, doc, &mut measured);
        // A text node that reaches the end of its line may continue on
        // the next line; the paragraph stays open for it.
        scan.run_open = !tail.trim().is_empty();
    }

    // A line whose only content sits inside an exempt subtree is a code
    // line: recorded raw and exempt from TEXT002, like fence content.
    let in_code = saw_code_text && measured.trim().is_empty();
    doc.lines.push(StrippedLine {
        number,
        text: if in_code { text.to_string() } else { measured },
        in_code_block: in_code,
    });
}

/// Applies one closed tag to the scan state: `<code>` and `<example>`
/// openings enter an exempt subtree, their closings leave it, and a
/// self-closing tag opens nothing.
fn close_tag(fragment: &str, scan: &mut TagScan) {
    let Some((name, closing, self_closing)) = tag_kind(fragment) else {
        return;
    };
    if !matches!(name, "code" | "example") {
        return;
    }
    if closing {
        scan.exempt = scan.exempt.saturating_sub(1);
    } else if !self_closing {
        scan.exempt += 1;
    }
}

/// Adds one text node to the measured line text and the open paragraph.
///
/// A whitespace-only node ends the paragraph like a blank line.
fn measure_segment(
    segment: &str,
    number: usize,
    scan: &TagScan,
    pending: &mut Option<PendingParagraph>,
    doc: &mut Document,
    measured: &mut String,
) {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        flush(pending, doc);
        return;
    }
    measured.push_str(segment);
    if !scan.run_open {
        flush(pending, doc);
    }
    match pending.as_mut() {
        Some(open) => {
            open.len += trimmed.chars().count();
            open.count += 1;
        }
        None => {
            *pending = Some(PendingParagraph {
                kind: ParagraphKind::Plain,
                first_line: number,
                len: trimmed.chars().count(),
                count: 1,
            });
        }
    }
}

/// The `(name, closing, self-closing)` classification of one tag's inner
/// fragment, or `None` when the fragment holds no well-formed name.
///
/// A tag whose `<` sits on an earlier line classifies from its closing
/// line's fragment, which keeps `</code>` detection honest in idiomatic
/// tag-per-line docs.
fn tag_kind(fragment: &str) -> Option<(&str, bool, bool)> {
    let (closing, rest) = match fragment.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, fragment),
    };
    let rest = rest.trim_start();
    let self_closing = rest.ends_with('/');
    let name_end = rest
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(rest.len());
    let name = &rest[..name_end];
    let well_formed = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'));
    well_formed.then_some((name, closing, self_closing))
}

#[cfg(test)]
mod tests {
    use crate::check::CODE_LINE_LENGTH;
    use crate::check::CODE_PARAGRAPH_SIZE;
    use crate::check::plaintext::region::{Dialect, DocRegion, RegionLine};
    use crate::check::plaintext::run_region_checks;
    use crate::check::plaintext::tests::codes;
    use crate::diagnostic::Diagnostic;

    /// Builds one XML-doc region from `(line number, text)` pairs.
    fn xml_region(lines: &[(usize, &str)]) -> DocRegion {
        DocRegion {
            dialect: Dialect::XmlDoc,
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

    /// Runs the text checks over one XML-doc region.
    fn measure(lines: &[(usize, &str)]) -> Vec<Diagnostic> {
        run_region_checks(vec![xml_region(lines)])
    }

    // ── Tag and attribute stripping ──

    // TEXT002 measures the inner text only: tags and attribute values
    // vanish from the measured line.
    #[test]
    fn xml_line_length_counts_inner_text_only() {
        let long = "x".repeat(81);
        let short = "y".repeat(79);
        let diags = measure(&[
            (
                1,
                &format!(r#"<param name="a very long attribute value">{long}</param>"#),
            ),
            (
                2,
                &format!(r#"<param name="a very long attribute value">{short}</param>"#),
            ),
        ]);

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1, "exactly the inner-text line warns");
        assert_eq!(found[0].line, 1);
    }

    // Text nodes on both sides of a tag join into one measured line with
    // the tag removed.
    #[test]
    fn xml_line_length_joins_text_around_inline_tags() {
        let head = "a".repeat(60);
        let tail = "b".repeat(25);
        let diags = measure(&[(3, &format!("Use <c>code</c> {head} {tail} now"))]);

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    // ── Paragraph segmentation ──

    // Prose never joins across a tag boundary: two texts that would
    // overflow the budget when joined stay silent inside their own tags.
    #[test]
    fn xml_paragraphs_never_join_across_tags() {
        let half = "word ".repeat(30);
        let param = half.trim();
        let diags = measure(&[
            (1, "<summary>"),
            (2, &half),
            (3, "</summary>"),
            (4, &format!(r#"<param name="x">{param}</param>"#)),
        ]);

        // Each half is exactly 149 chars (5 * 29 + 4), under the limit.
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // A text node that reaches the end of its line continues on the next
    // line: the joined paragraph reports at its first line.
    #[test]
    fn xml_paragraph_joins_contiguous_text_lines() {
        let filler = "z".repeat(120);
        let diags = measure(&[
            (2, "<summary>"),
            (3, &filler),
            (4, &filler),
            (5, "</summary>"),
        ]);

        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3, "the paragraph reports at its first line");
    }

    // A whitespace-only text node splits the paragraph like a blank line.
    #[test]
    fn xml_blank_node_splits_the_paragraph() {
        let filler = "z".repeat(120);
        let diags = measure(&[
            (1, "<remarks>"),
            (2, &filler),
            (3, ""),
            (4, &filler),
            (5, "</remarks>"),
        ]);

        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // Text outside any tag is plain doc prose and is measured.
    #[test]
    fn xml_measures_untagged_prose() {
        let filler = "z".repeat(241);
        let diags = measure(&[(7, &filler)]);

        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 7);
    }

    // ── Exempt subtrees ──

    // `<code>` and `<example>` subtrees are exempt like code fences:
    // their lines never warn on length or feed a paragraph, including a
    // nested `<code>` inside `<example>`.
    #[test]
    fn xml_code_and_example_subtrees_are_exempt() {
        let long = "c".repeat(90);
        let para = "p".repeat(130);
        let diags = measure(&[
            (1, "<example>"),
            (2, &para),
            (3, "<code>"),
            (4, &long),
            (5, &format!("if (len < {long}.len()) {{ return; }}")),
            (6, "</code>"),
            (7, &para),
            (8, "</example>"),
            (9, "<summary>done</summary>"),
        ]);

        assert!(diags.is_empty(), "exempt subtree stays fully quiet");
    }

    // A `<code>` that closes after the opening tag's line still leaves
    // the subtree at its closing tag.
    #[test]
    fn xml_code_subtree_spans_lines() {
        let long = "c".repeat(90);
        let diags = measure(&[(1, "<code>"), (2, &long), (3, "</code>"), (4, &long)]);

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1, "only the post-code line warns");
        assert_eq!(found[0].line, 4);
    }

    // A self-closing `<code/>` opens no subtree: the text after it is
    // measured prose.
    #[test]
    fn xml_self_closing_code_opens_no_subtree() {
        let long = "p".repeat(81);
        let diags = measure(&[(2, &format!("<code/> {long}"))]);

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
    }
}
