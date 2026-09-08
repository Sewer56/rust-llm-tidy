//! Plaintext extraction and measurement: doc regions fold into the
//! stripped lines and paragraphs the text rules measure.
//!
//! Doc-region producers strip a file's comment markers into
//! [`region::DocRegion`]s. The measuring core folds them into numbered
//! stripped doc lines, paragraphs, and exemption classifications in one
//! linear pass.
//!
//! Each region is measured with its dialect's rules, so markdown prose
//! and XML doc comments feed the same measurement.
//!
//! The measured budgets count the full line text, code spans, URLs, and
//! link targets included. Decorative borders, table rows, code blocks,
//! and link reference definitions are exempt.
//!
//! # Layers
//!
//! - [`region`] - the doc-region input shape: stripped lines, original
//!   line numbers, and the dialect tag.
//! - [`line_markers`] - the legacy producer: line-comment markers keyed by
//!   file extension, one region per contiguous comment run. The Rust AST
//!   producer reuses its `rs` regions through [`line_marker_regions`].
//!
//! Dialect adapters:
//!
//! - [`xml_doc`] - the XML doc dialect: text-node measurement over
//!   tag-carrying doc lines.
//! - [`block_doc`] - the block doc dialect: `*`-continuation stripping and
//!   `@tag` exemption over `/** */`-style doc lines.
//! - [`docstring`] - the docstring dialect: markdown prose over Python
//!   docstring lines with `>>>` doctest examples exempt.
//!
//! Measurement and output:
//!
//! - [`analyze`] - producer plus measuring core over one file.
//! - [`measure`] - the measuring core over explicit region lists.
//! - [`Paragraph`] - a measured paragraph: plain text or a bullet with its
//!   wrapped continuations.

pub use line_markers::doc_regions as line_marker_regions;
pub use region::{Dialect, DocRegion, RegionLine};

mod block_doc;
mod docstring;
mod line_markers;
pub(crate) mod region;
mod xml_doc;

/// Stripped lines and paragraphs extracted from one file.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Document {
    pub lines: Vec<StrippedLine>,
    pub paragraphs: Vec<Paragraph>,
    /// Opening fence lines, in source order. Recorded for every
    /// opening fence the markdown classifier measures, whole-file
    /// and doc/comment regions alike.
    pub fences: Vec<Fence>,
}

/// The open fence of a code block: its marker character and run length.
///
/// Only a later line with the same marker, an equal-or-longer run, and no
/// info string closes it; shorter or different-marker fences stay content.
#[derive(Debug, PartialEq, Eq)]
struct OpenFence {
    marker: u8,
    run: usize,
}

/// The paragraph under construction between boundary lines. Member texts
/// accumulate joined with single spaces, exactly as [`flush`] publishes
/// them.
struct PendingParagraph {
    kind: ParagraphKind,
    /// 1-based line number of the paragraph's first member line.
    first_line: usize,
    /// Member texts so far, joined with single spaces.
    text: String,
    /// Each member line's line number and byte offset in `text`, in
    /// member order.
    line_starts: Vec<(usize, usize)>,
}

/// An opening fence line of a fenced code block.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Fence {
    /// 1-based line number of the opening fence line.
    pub line: usize,
    /// The fence line's info string: the text after the fence marker
    /// run, surrounding whitespace trimmed.
    pub info: Box<str>,
}

/// A measured paragraph: the member lines' trimmed text joined with single
/// spaces; exempt lines are never members, so they cost nothing.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Paragraph {
    /// 1-based line number of the paragraph's first member line.
    pub first_line: usize,
    /// True when this is the first paragraph of a doc region: the region's
    /// opener, such as a module or method doc's leading paragraph.
    pub opens_region: bool,
    pub kind: ParagraphKind,
    /// Char count of `text`.
    pub size: usize,
    /// The member lines' trimmed text joined with single spaces; the
    /// sentence-length rule splits it into sentences.
    pub text: Box<str>,
    /// Each member line's line number and byte offset in `text`, in
    /// member order; the sentence-length rule anchors findings at these
    /// lines.
    pub line_starts: Vec<(usize, usize)>,
}

/// A doc or comment line after prefix/indent stripping, with its 1-based
/// original line number.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StrippedLine {
    pub number: usize,
    pub text: String,
    /// True inside fenced or indented code blocks, fence delimiters
    /// included. Code blocks are exempt from both checks.
    pub in_code_block: bool,
}

/// Whether a paragraph is plain text or a bullet with wrapped continuations.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParagraphKind {
    /// Consecutive text lines up to a blank or exempt boundary line.
    Plain,
    /// A bullet marker line plus its wrapped continuation lines.
    Bullet,
}

impl PendingParagraph {
    /// Starts a paragraph whose first member line is `line` with `text`.
    fn new(kind: ParagraphKind, line: usize, text: &str) -> Self {
        Self {
            kind,
            first_line: line,
            text: text.to_string(),
            line_starts: vec![(line, 0)],
        }
    }

    /// Appends one member line's trimmed `text` at `line`.
    fn push_member(&mut self, line: usize, text: &str) {
        self.text.push(' ');
        self.line_starts.push((line, self.text.len()));
        self.text.push_str(text);
    }
}

/// Strips and segments `source` for the given file extension.
///
/// The [`line_markers`] producer builds the file's doc regions and
/// [`measure`] folds them into the document. Lines without a matching
/// comment marker are skipped entirely for marker languages; for
/// marker-less extensions every line is kept.
pub(crate) fn analyze(source: &str, ext: &str) -> Document {
    measure(line_markers::doc_regions(source, ext))
}

/// Identifies standalone decorative borders without exempting their labels.
///
/// A border has at least three drawing characters, optionally spaced.
/// ASCII separators and Unicode box-drawing/block characters may mix.
/// Markdown fence classification takes precedence over borders.
pub(crate) fn is_decorative_border(text: &str) -> bool {
    let mut count = 0;
    for ch in text.chars().filter(|ch| !ch.is_whitespace()) {
        if !matches!(
            ch,
            '-' | '=' | '_' | '*' | '+' | '|' | '/' | '\\' | '.' | ':' | '#' | '~' | '\u{2500}'
                ..='\u{259f}'
        ) {
            return false;
        }
        count += 1;
    }

    count >= 3
}

/// True for markdown link reference definitions such as `[docs]: ./docs/x.md`.
///
/// Used by the exempt-content classifier here and by the TEXT002 line
/// length rule in `crate::rules::lint::text`.
pub(crate) fn is_link_reference_definition(trimmed: &str) -> bool {
    trimmed.starts_with('[') && trimmed.contains("]:")
}

/// Folds `regions` into one [`Document`] in a single linear pass.
///
/// Each region is measured with its dialect's rules. The gap between two
/// regions ends any open paragraph and closes any open fence, so prose and
/// code blocks never span regions.
///
/// Each markdown-measured opening fence lands in [`Document::fences`],
/// feeding TEXT005 in every tier.
pub(crate) fn measure(regions: Vec<DocRegion>) -> Document {
    let mut doc = Document::default();
    let mut pending: Option<PendingParagraph> = None;
    let mut open_fence: Option<OpenFence> = None;

    for region in regions {
        let region_start = doc.paragraphs.len();
        match region.dialect {
            Dialect::Markdown => {
                measure_markdown_region(region, &mut doc, &mut pending, &mut open_fence);
            }
            Dialect::XmlDoc => {
                xml_doc::measure_region(region, &mut doc, &mut pending);
            }
            Dialect::BlockDoc => {
                block_doc::measure_region(region, &mut doc, &mut pending, &mut open_fence);
            }
            Dialect::Docstring => {
                docstring::measure_region(region, &mut doc, &mut pending, &mut open_fence);
            }
        }
        // A region break is a gap of non-doc lines: paragraphs and fences
        // never span it.
        flush(&mut pending, &mut doc);
        open_fence = None;
        // The region's first paragraph is its opener.
        if let Some(first) = doc.paragraphs.get_mut(region_start) {
            first.opens_region = true;
        }
    }
    doc
}

/// Measures one markdown-prose region: the producer already stripped the
/// comment markers, so each line goes through the shared prose classifier.
///
/// `open_fence` carries the open-fence state in and out: a fence opened here
/// stays open until a matching closing fence line or the region's end.
fn measure_markdown_region(
    region: DocRegion,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
    open_fence: &mut Option<OpenFence>,
) {
    for line in region.lines {
        measure_prose_line(
            line.text,
            line.number,
            line.indented,
            doc,
            pending,
            open_fence,
        );
    }
}

/// Classifies and measures one prose line under the markdown rules: fence
/// tracking, indented-code and exempt-content classification, and bullet
/// segmentation.
///
/// Shared by the markdown dialect (marker-stripped doc lines) and the
/// [`block_doc`] dialect (`*`-continuation-stripped block lines); `indented`
/// is the caller's indented-code fact for the measured text.
///
/// [`block_doc`]: self::block_doc
fn measure_prose_line(
    text: String,
    number: usize,
    indented: bool,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
    open_fence: &mut Option<OpenFence>,
) {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        flush(pending, doc);
        doc.lines.push(StrippedLine {
            number,
            text,
            in_code_block: false,
        });
        return;
    }

    // Decide whether this line is exempt from paragraph measuring; the
    // fence-matching rules live on `fence_delimiter` and `closes_fence`.
    //
    // Fence lines and everything between them are exempt. Outside a
    // block, indented code and lines like headings, tables, and
    // signature-like lines (full list on `is_exempt_content`) are also
    // exempt.
    let fence = fence_delimiter(trimmed);
    let in_code_block = open_fence.is_some() || fence.is_some() || indented;
    let exempt = if let Some(open) = open_fence.as_ref() {
        // An indented line is indented code, not a fence delimiter, so
        // only an unindented delimiter can close the open fence.
        if !indented && closes_fence(open, fence, trimmed) {
            *open_fence = None;
        }
        true
    } else if fence.is_some() || indented || is_exempt_content(trimmed) {
        // An indented line is indented code, not a fence delimiter:
        // fence state only changes on unindented fence lines.
        //
        // Per CommonMark, a backtick fence's info string may not
        // contain backticks; such a line is not a fence opener.
        if let Some((marker, run)) = fence.filter(|_| !indented) {
            let info = fence_info(trimmed);
            if marker == b'~' || !info.contains('`') {
                *open_fence = Some(OpenFence { marker, run });
                doc.fences.push(Fence {
                    line: number,
                    info: info.into(),
                });
            }
        }
        true
    } else {
        false
    };

    // Count this line into the current paragraph (`pending`) or start
    // a new one.
    //
    // A paragraph is a run of consecutive doc lines, ended by a blank
    // line, an exempt line, or the start of a new bullet.
    if exempt {
        // Exempt lines are not paragraph text, so this is the end
        // of the current paragraph.
        flush(pending, doc);
    } else if let Some(content) = bullet_content(trimmed) {
        // A bullet ends the current paragraph and starts its own,
        // measured from the text after the bullet marker.
        flush(pending, doc);
        *pending = Some(PendingParagraph::new(
            ParagraphKind::Bullet,
            number,
            content,
        ));
    } else if let Some(open) = pending.as_mut() {
        // Continuation lines (next plain line or wrapped bullet tail)
        // join the current paragraph; only trimmed text counts.
        open.push_member(number, trimmed);
    } else {
        // Plain text with no paragraph open: start one at this line.
        *pending = Some(PendingParagraph::new(ParagraphKind::Plain, number, trimmed));
    }
    doc.lines.push(StrippedLine {
        number,
        text,
        in_code_block,
    });
}

/// The paragraph text after the bullet marker, or `None` for non-bullets.
///
/// Recognized bullet forms:
///
/// ```text
/// - dash
/// * asterisk
/// + plus
/// 1. ordered with a dot
/// 2) ordered with a parenthesis
/// ```
fn bullet_content(trimmed: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some(rest);
        }
    }
    let digits_end = trimmed.find(['.', ')'])?;
    if digits_end > 0
        && trimmed[..digits_end].chars().all(|c| c.is_ascii_digit())
        && (trimmed[digits_end..].starts_with(". ") || trimmed[digits_end..].starts_with(") "))
    {
        Some(&trimmed[digits_end + 2..])
    } else {
        None
    }
}

/// Whether `trimmed` closes the open fence `open`.
///
/// Closing requires the same marker, an equal-or-longer run, and no
/// info string. Shorter, different-marker, or info-bearing fence lines
/// stay fenced content.
fn closes_fence(open: &OpenFence, fence: Option<(u8, usize)>, trimmed: &str) -> bool {
    match fence {
        Some((marker, run)) => {
            marker == open.marker && run >= open.run && fence_info(trimmed).is_empty()
        }
        None => false,
    }
}

/// The fence delimiter's marker character and run length, or `None` for
/// non-fence lines. A delimiter is a lead of three or more backticks or
/// tildes.
fn fence_delimiter(trimmed: &str) -> Option<(u8, usize)> {
    let bytes = trimmed.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let run = bytes.iter().take_while(|&&b| b == marker).count();
    (run >= 3).then_some((marker, run))
}

/// Folds the accumulated member texts into a finished paragraph, if any.
fn flush(pending: &mut Option<PendingParagraph>, doc: &mut Document) {
    if let Some(open) = pending.take() {
        let size = open.text.chars().count();
        doc.paragraphs.push(Paragraph {
            first_line: open.first_line,
            opens_region: false,
            kind: open.kind,
            size,
            text: open.text.into_boxed_str(),
            line_starts: open.line_starts,
        });
    }
}

/// Excludes non-prose lines from paragraph budgets and ends open paragraphs.
///
/// Exempt lines:
/// - Headings and table rows
/// - Signature-like lines and link reference definitions
/// - Standalone decorative borders
///
/// Code spans and URLs are not whole-line exemptions; those lines stay
/// paragraph members whose full text counts toward the budget.
fn is_exempt_content(trimmed: &str) -> bool {
    trimmed.starts_with('#')
        || trimmed.starts_with('|')
        || is_signature_line(trimmed)
        || is_link_reference_definition(trimmed)
        || is_decorative_border(trimmed)
}

/// The opening fence's info string: the text after the fence marker run,
/// surrounding whitespace trimmed.
///
/// `trimmed` starts with at least three fence characters; a longer marker
/// run (` ```` `) belongs to the marker, not the info string.
fn fence_info(trimmed: &str) -> &str {
    let marker = trimmed.as_bytes()[0];
    let run = trimmed.bytes().take_while(|&b| b == marker).count();
    trimmed[run..].trim()
}

/// True for lines that look like code signatures rather than plain text.
///
/// Signature keywords must start the line, after any Rust visibility
/// modifier; keyword mentions inside prose stay measured.
fn is_signature_line(trimmed: &str) -> bool {
    for keyword in ["fn ", "struct ", "enum ", "trait ", "impl "] {
        if starts_with_signature_keyword(trimmed, keyword) {
            return true;
        }
    }
    trimmed.ends_with(';')
        || trimmed.ends_with('{')
        || trimmed.ends_with('(')
        || trimmed.ends_with("->")
}

/// True when `keyword` starts `line`, after any Rust visibility modifier
/// (`pub`, `pub(crate)`, `pub(in path)`).
fn starts_with_signature_keyword(line: &str, keyword: &str) -> bool {
    let Some(after_pub) = line.strip_prefix("pub") else {
        return line.starts_with(keyword);
    };
    let after_visibility = match after_pub.strip_prefix('(') {
        Some(inner) => inner.split_once(')').map_or("", |(_, after)| after),
        None => after_pub,
    };
    after_visibility.trim_start().starts_with(keyword)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    /// Number of the paragraph that starts at `line`, if present.
    fn paragraph_at(doc: &Document, line: usize) -> Option<&Paragraph> {
        doc.paragraphs.iter().find(|p| p.first_line == line)
    }

    // ── Prefix and indent stripping ──

    // `///` with space and tab indents strips to the bare text.
    #[test]
    fn analyze_strips_doc_comment_marker_and_indent() {
        let doc = analyze("    /// text\n\t/// more\n", "rs");
        assert_eq!(doc.lines[0].text, "text");
        assert_eq!(doc.lines[1].text, "more");
        assert_eq!(doc.lines[0].number, 1);
    }

    // Only leading whitespace, the marker, and at most one space go away.
    #[test]
    fn analyze_strips_at_most_one_space_after_marker() {
        let doc = analyze("//  two spaces kept\n", "rs");
        assert_eq!(doc.lines[0].text, " two spaces kept");
    }

    // `//` and `//!` markers strip like `///`.
    #[test]
    fn analyze_strips_all_rust_markers() {
        let source = indoc! {"
            // a
            //! b
            /// c
        "};
        let doc = analyze(source, "rs");
        let texts: Vec<&str> = doc.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b", "c"]);
    }

    // CRLF endings are handled: no stray `\r` survives into the text.
    #[test]
    fn analyze_handles_crlf_endings() {
        let doc = analyze("/// alpha\r\n///\r\n/// beta\r\n", "rs");
        assert_eq!(doc.lines[0].text, "alpha");
        assert_eq!(doc.lines[1].text, "");
        assert_eq!(doc.lines[2].text, "beta");
        assert_eq!(doc.paragraphs.len(), 2);
    }

    // Rust lines without a comment marker are not doc lines at all.
    #[test]
    fn analyze_skips_non_comment_rust_lines() {
        let source = indoc! {"
            let x = 1;
            // note
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.lines.len(), 1);
        assert_eq!(doc.lines[0].text, "note");
    }

    // ── Marker table ──

    // Marker-less extensions (the markdown family) measure every line.
    #[test]
    fn analyze_keeps_all_markdown_lines() {
        let source = indoc! {"
            # Title

            Paragraph text.
        "};
        let doc = analyze(source, "md");
        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.lines[0].text, "# Title");
    }

    // ── Paragraph segmentation ──

    #[test]
    fn analyze_should_split_paragraphs_when_borders_separate_prose() {
        let source = "// ---\n// Opener.\n// * * *\n// Body.\n// ╚══╝\n";

        let doc = analyze(source, "rs");

        assert_eq!(doc.paragraphs.len(), 2);
        let opener = &doc.paragraphs[0];
        assert_eq!(&*opener.text, "Opener.");
        assert_eq!(opener.size, "Opener.".len());
        assert_eq!(opener.line_starts, [(2, 0)]);
        assert!(opener.opens_region);
        let body = &doc.paragraphs[1];
        assert_eq!(&*body.text, "Body.");
        assert_eq!(body.line_starts, [(4, 0)]);
        assert!(!body.opens_region);
    }

    #[test]
    fn is_decorative_border_should_require_drawing_characters_without_prose() {
        for (text, expected) in [
            ("", false),
            ("  ", false),
            ("--", false),
            ("---", true),
            (" * * * ", true),
            ("+-=+", true),
            ("╭──╮", true),
            ("░▒▓", true),
            ("--- Shared path resolution ---", false),
            ("--- Résolution ---", false),
            ("123", false),
            ("!?", false),
        ] {
            assert_eq!(is_decorative_border(text), expected, "{text:?}");
        }
    }

    // Blank lines split paragraphs; size joins lines with single spaces.
    #[test]
    fn analyze_splits_paragraphs_at_blank_lines() {
        let source = indoc! {"
            /// one two
            /// three

            /// four
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 2);
        let first = paragraph_at(&doc, 1).unwrap();
        assert_eq!(first.kind, ParagraphKind::Plain);
        assert_eq!(first.size, "one two three".len());
        assert_eq!(paragraph_at(&doc, 4).unwrap().size, "four".len());
    }

    // A non-doc source line between doc lines ends the open paragraph: prose
    // never joins across the code gap.
    #[test]
    fn analyze_splits_paragraphs_at_non_doc_lines() {
        let source = indoc! {"
            /// one two
            let x = 1;
            /// three
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "one two".len());
        assert_eq!(paragraph_at(&doc, 3).unwrap().size, "three".len());
    }

    // ── Retained paragraph text ──

    // Each doc region's first paragraph is its opener; later paragraphs in
    // the region are not.
    #[test]
    fn analyze_marks_each_regions_first_paragraph_as_opener() {
        let source = indoc! {"
            /// first region opener
            ///
            /// first region follower

            /// second region opener
        "};
        let doc = analyze(source, "rs");
        assert!(paragraph_at(&doc, 1).unwrap().opens_region);
        assert!(!paragraph_at(&doc, 3).unwrap().opens_region);
        assert!(paragraph_at(&doc, 5).unwrap().opens_region);
    }

    // The finished paragraph retains its joined full text and each member
    // line's number with its start offset in that text.
    #[test]
    fn analyze_retains_paragraph_text_and_member_lines() {
        let source = indoc! {"
            /// one two
            /// three

            /// four
        "};
        let doc = analyze(source, "rs");
        let first = paragraph_at(&doc, 1).unwrap();
        assert_eq!(&*first.text, "one two three");
        assert_eq!(first.size, "one two three".len());
        assert_eq!(first.line_starts, vec![(1, 0), (2, 8)]);
        assert_eq!(&*paragraph_at(&doc, 4).unwrap().text, "four");
    }

    // A bullet retains its text without the bullet marker, wrapped
    // continuation included.
    #[test]
    fn analyze_retains_bullet_text_without_the_marker() {
        let source = indoc! {"
            /// - bullet start
            ///   wrapped tail
        "};
        let doc = analyze(source, "rs");
        let bullet = paragraph_at(&doc, 1).unwrap();
        assert_eq!(&*bullet.text, "bullet start wrapped tail");
        assert_eq!(bullet.line_starts, vec![(1, 0), (2, 13)]);
    }

    // ── Bullets ──

    // A bullet plus wrapped continuation is its own paragraph.
    #[test]
    fn analyze_groups_bullet_with_wrapped_continuation() {
        let source = indoc! {"
            /// intro prose
            /// - bullet start
            ///   wrapped tail
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 2);
        let bullet = paragraph_at(&doc, 2).unwrap();
        assert_eq!(bullet.kind, ParagraphKind::Bullet);
        assert_eq!(bullet.size, "bullet start wrapped tail".len());
    }

    // Nested bullets are separate paragraphs, excluded from the parent bullet
    // and from any enclosing text paragraph.
    #[test]
    fn analyze_separates_nested_bullets() {
        let source = indoc! {"
            prose line
            - top bullet
              - nested bullet
        "};
        let doc = analyze(source, "md");
        assert_eq!(doc.paragraphs.len(), 3);
        assert_eq!(paragraph_at(&doc, 1).unwrap().kind, ParagraphKind::Plain);
        let top = paragraph_at(&doc, 2).unwrap();
        let nested = paragraph_at(&doc, 3).unwrap();
        assert_eq!(top.kind, ParagraphKind::Bullet);
        assert_eq!(nested.kind, ParagraphKind::Bullet);
        assert_eq!(top.size, "top bullet".len());
        assert_eq!(nested.size, "nested bullet".len());
    }

    // Ordered `1. ` bullets identify like dash bullets.
    #[test]
    fn analyze_identifies_ordered_bullets() {
        let source = indoc! {"
            1. first
            2. second
        "};
        let doc = analyze(source, "md");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().kind, ParagraphKind::Bullet);
    }

    // ── Fence capture ──

    // The measuring core records each opening fence's line and trimmed
    // info string; closing fences and tagged fences are facts, not
    // findings.
    #[test]
    fn analyze_records_opening_fence_info_strings() {
        let source = indoc! {"
            intro

            ```rust
            code
            ```

            ~~~ ignore
            code
            ~~~
        "};
        let doc = analyze(source, "md");
        assert_eq!(
            doc.fences,
            vec![
                Fence {
                    line: 3,
                    info: "rust".into(),
                },
                Fence {
                    line: 7,
                    info: "ignore".into(),
                },
            ]
        );
    }

    // A backtick fence whose info string contains a backtick is not a
    // fence opener (CommonMark).
    //
    // No fence is recorded and the fence state stays closed. Tilde
    // fences may carry backticks in the info.
    #[test]
    fn analyze_skips_fence_opener_when_info_has_backtick() {
        let source = indoc! {"
            ``` not a`fence
            text
            ~~~ has`backticks
            code
            ~~~
        "};
        let doc = analyze(source, "md");
        assert_eq!(
            doc.fences,
            vec![Fence {
                line: 3,
                info: "has`backticks".into(),
            }]
        );
        // The invalid backtick line is exempt content (still a fence-
        // shaped line), so no paragraph opens for `text`.
    }

    // A longer open fence stays open across shorter or different-marker
    // inner fences: only a matching, info-free fence line closes it.
    #[test]
    fn analyze_keeps_longer_fence_open_across_inner_fences() {
        let source = indoc! {"
            ~~~~markdown
            ```text
            inner
            ```
            still code
            ~~~~

            after
        "};
        let doc = analyze(source, "md");
        assert_eq!(
            doc.fences,
            vec![Fence {
                line: 1,
                info: "markdown".into(),
            }]
        );
        // Everything between the outer delimiters is code-block content,
        // the nested fence included.
        for number in 1..=6 {
            let line = doc.lines.iter().find(|l| l.number == number).unwrap();
            assert!(line.in_code_block, "line {number} must be fenced content");
        }
        // Prose after the block still measures.
        assert_eq!(paragraph_at(&doc, 8).unwrap().size, "after".len());
    }

    // A same-marker inner fence with a shorter run or an info string
    // never closes the outer block.
    //
    // Built by concatenation so this source file keeps the canonical
    // outer-backtick/inner-tilde alternation.
    #[test]
    fn analyze_keeps_fence_open_across_shorter_and_info_bearing_closers() {
        let inner_open = concat!("``", "`text");
        let inner_close = concat!("``", "`");
        let info_closer = concat!("```", "` tail");
        let source = format!(
            "````markdown\n{inner_open}\ninner\n{inner_close}\n{info_closer}\nstill code\n````\nafter\n"
        );
        let doc = analyze(&source, "md");
        assert_eq!(
            doc.fences,
            vec![Fence {
                line: 1,
                info: "markdown".into(),
            }]
        );
        // The shorter inner fence and the info-bearing equal-run line both
        // stay fenced content; only the bare four-backtick line closes.
        for number in 1..=7 {
            let line = doc.lines.iter().find(|l| l.number == number).unwrap();
            assert!(line.in_code_block, "line {number} must be fenced content");
        }
        assert_eq!(paragraph_at(&doc, 8).unwrap().size, "after".len());
    }

    // Doc-comment regions record fences too: a bare fence in Rust doc
    // comments is a markdown fence the same rule grades.
    //
    // Built with `concat!` so the fence lines stay string fragments,
    // not measured comment lines.
    #[test]
    fn analyze_records_fences_from_doc_comment_regions() {
        let bare_fence = concat!("``", "`");
        let source = format!("/// {bare_fence}\n/// let x = 1;\n/// {bare_fence}\nfn f() {{}}\n");
        assert_eq!(
            analyze(&source, "rs").fences,
            vec![Fence {
                line: 1,
                info: "".into(),
            }]
        );
        assert!(measure(vec![]).fences.is_empty());
    }

    // ── Exemptions ──

    // Fenced code content is exempt: it forms no paragraph.
    #[test]
    fn analyze_exempts_fenced_code() {
        let source = indoc! {"
            text
            ~~~rust
            let x = 1;
            let y = 2;
            ~~~
            after
        "};
        let doc = analyze(source, "md");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "text".len());
        assert_eq!(paragraph_at(&doc, 6).unwrap().size, "after".len());
    }

    // A non-doc source line closes an open fence: doc lines after it are
    // measured as prose, not swallowed as code-block content.
    #[test]
    fn analyze_closes_fence_at_non_doc_line() {
        let source = indoc! {"
            /// ~~~text
            let x = 1;
            /// measured prose
        "};
        let doc = analyze(source, "rs");
        let line = doc.lines.iter().find(|l| l.number == 3).unwrap();
        assert!(!line.in_code_block);
        assert_eq!(paragraph_at(&doc, 3).unwrap().size, "measured prose".len());
    }

    // Tab- or 4-space-indented doc lines are exempt indented code.
    #[test]
    fn analyze_exempts_indented_code() {
        let source = indoc! {"
            /// prose
            ///
            ///     let x = 1;
            /// \tlet y = 2;
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "prose".len());
    }

    // Markdown 4-space raw indent is exempt indented code.
    #[test]
    fn analyze_exempts_raw_indented_markdown_code() {
        let doc = analyze("    indented code\nprose\n", "md");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(paragraph_at(&doc, 2).unwrap().size, "prose".len());
    }

    // An indented backtick line is indented code, never a fence opener:
    // prose after it still measures and no fence is recorded.
    #[test]
    fn analyze_measures_prose_after_an_indented_fence_lookalike() {
        let doc = analyze("    ```\nsurplus prose beyond any fence\n", "md");
        assert!(doc.fences.is_empty());
        let line = doc.lines.iter().find(|l| l.number == 1).unwrap();
        assert!(line.in_code_block);
        let prose = doc.lines.iter().find(|l| l.number == 2).unwrap();
        assert!(!prose.in_code_block);
        assert_eq!(
            paragraph_at(&doc, 2).unwrap().size,
            "surplus prose beyond any fence".len()
        );
    }

    // Table rows and headings are exempt content.
    #[test]
    fn analyze_exempts_tables_and_headings() {
        let source = indoc! {"
            | a | b |
            # Heading
        "};
        let doc = analyze(source, "md");
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.lines.len(), 2);
    }

    // Signature-like lines are exempt content.
    #[test]
    fn analyze_exempts_signature_lines() {
        let source = indoc! {"
            /// prose
            /// fn do_thing(x: usize) -> bool;
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "prose".len());
    }

    // A backtick-wrapped signature mention is prose, not a signature line:
    // it counts toward the paragraph budget in full.
    #[test]
    fn analyze_counts_backtick_wrapped_signature() {
        let doc = analyze("/// uses `fn do_thing(x: usize) -> bool;` here\n", "rs");
        assert_eq!(
            paragraph_at(&doc, 1).unwrap().size,
            "uses `fn do_thing(x: usize) -> bool;` here".len()
        );
    }

    // Signature keywords exempt only at the line start, after Rust
    // visibility modifiers.
    #[test]
    fn analyze_exempts_signature_keywords_at_line_start() {
        let source = indoc! {"
            /// fn compute(x: usize) -> usize
            /// pub struct Config
            /// pub(crate) enum Mode
            /// pub(in crate::base) trait Load
        "};
        let doc = analyze(source, "rs");
        assert!(doc.paragraphs.is_empty());
    }

    // Inline keyword mentions in prose are measured, not exempted.
    #[test]
    fn analyze_measures_prose_with_inline_signature_keywords() {
        let source = indoc! {"
            /// prose naming a struct or an impl block mid-line
            /// more prose about the enum
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        let para = paragraph_at(&doc, 1).unwrap();
        assert_eq!(
            para.size,
            "prose naming a struct or an impl block mid-line more prose about the enum".len()
        );
    }

    // Markdown link reference definitions are exempt content.
    #[test]
    fn analyze_exempts_link_reference_definitions() {
        let source = indoc! {"
            [docs]: ./docs/lints.md
            [cli]: ./src/cli/README.MD
        "};
        let doc = analyze(source, "md");
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.lines.len(), 2);
    }

    // A URL-bearing line stays a paragraph member: its full text counts,
    // and the paragraph does not split at it.
    #[test]
    fn analyze_joins_paragraph_across_url_line() {
        let source = indoc! {"
            /// first part
            /// see https://example.com/x
            /// second part
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(
            paragraph_at(&doc, 1).unwrap().size,
            "first part see https://example.com/x second part".len()
        );
    }

    // A mixed text + code-span line is a paragraph member whose full text
    // counts, spans included.
    #[test]
    fn analyze_counts_mixed_line_including_code_span() {
        let doc = analyze("/// run `cargo test` to verify\n", "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        let para = paragraph_at(&doc, 1).unwrap();
        assert_eq!(para.kind, ParagraphKind::Plain);
        assert_eq!(para.size, "run `cargo test` to verify".len());
    }

    // A span-only line is a normal member: it counts in full and joins
    // with single spaces.
    #[test]
    fn analyze_counts_span_only_line_in_paragraph() {
        let source = indoc! {"
            /// alpha
            /// `cargo test`
            /// omega
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(
            paragraph_at(&doc, 1).unwrap().size,
            "alpha `cargo test` omega".len()
        );
    }

    // Link text and link targets both count toward the budget.
    #[test]
    fn analyze_counts_link_text_and_targets() {
        let doc = analyze(
            "/// see [docs](./docs/lints.md) and [guide][ref] here\n",
            "rs",
        );
        assert_eq!(
            paragraph_at(&doc, 1).unwrap().size,
            "see [docs](./docs/lints.md) and [guide][ref] here".len()
        );
    }
}
