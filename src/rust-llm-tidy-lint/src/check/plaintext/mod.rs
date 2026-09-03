//! Plaintext extraction, segmentation, and the paragraph and line-length
//! checks built on them.
//!
//! Doc-region producers strip a file's comment markers into
//! [`region::DocRegion`]s; the measuring core folds them into numbered
//! stripped doc lines, paragraphs, and exemption classifications in one
//! linear pass.
//!
//! Both checks count the full line text, code spans, URLs, and link targets
//! included; table rows, code blocks, and link reference definitions are
//! exempt.
//!
//! # Layers
//!
//! - [`region`] - the doc-region input shape: stripped lines, original
//!   line numbers, and the dialect tag.
//! - [`line_markers`] - the legacy producer: line-comment markers keyed by
//!   file extension, one region per contiguous comment run.
//! - [`analyze`] - producer plus measuring core over one file.
//! - [`measure`] - the measuring core over explicit region lists.
//! - [`Paragraph`] - a measured paragraph: plain text or a bullet with its
//!   wrapped continuations.
//! - [`run_text_checks`] - DOC007/DOC008 over the analysis result, delegated
//!   to [`paragraph_length`] and [`line_length`].

use crate::diagnostic::Diagnostic;
use region::{Dialect, DocRegion};

mod line_length;
mod line_markers;
mod paragraph_length;
mod region;

/// Stripped lines and paragraphs extracted from one file.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Document {
    pub lines: Vec<StrippedLine>,
    pub paragraphs: Vec<Paragraph>,
}

/// The paragraph under construction between boundary lines. `len` sums the
/// member char counts without joining spaces; [`flush`] adds one joining
/// space per extra member, derived from `count`.
struct PendingParagraph {
    kind: ParagraphKind,
    /// 1-based line number of the paragraph's first member line.
    first_line: usize,
    /// Summed char count of the member lines so far.
    len: usize,
    /// Number of member lines so far.
    count: usize,
}

/// A measured paragraph. `size` is the full member text joined with single
/// spaces; exempt lines are never members, so they cost nothing.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Paragraph {
    /// 1-based line number of the paragraph's first member line.
    pub first_line: usize,
    pub kind: ParagraphKind,
    pub size: usize,
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

/// Runs DOC007 and DOC008 over one file's raw text.
///
/// DOC007 fires an Error when a plain paragraph's size exceeds 240 chars,
/// and a Warning when a bullet's does; DOC008 fires a Warning for every
/// line over 80 chars.
///
/// Both count the full line text; table rows, code blocks, and link
/// reference definitions are exempt.
///
/// # Arguments
///
/// - `source` - the raw file text.
/// - `ext` - the file extension, selecting the comment marker table.
///
/// # Returns
///
/// Diagnostics in source order: DOC007 per over-limit paragraph (bullet
/// warnings after their paragraph position), then DOC008 per over-limit line.
pub fn run_text_checks(source: &str, ext: &str) -> Vec<Diagnostic> {
    let doc = analyze(source, ext);
    let mut diags = paragraph_length::diagnostics(&doc);
    diags.extend(line_length::diagnostics(&doc));
    diags
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

/// Folds `regions` into one [`Document`] in a single linear pass.
///
/// Each region is measured with its dialect's rules. The gap between two
/// regions ends any open paragraph and closes any open fence, so prose and
/// code blocks never span regions.
pub(crate) fn measure(regions: Vec<DocRegion>) -> Document {
    let mut doc = Document::default();
    let mut pending: Option<PendingParagraph> = None;
    let mut in_fence = false;

    for region in regions {
        match region.dialect {
            Dialect::Markdown => {
                measure_markdown_region(region, &mut doc, &mut pending, &mut in_fence);
            }
        }
        // A region break is a gap of non-doc lines: paragraphs and fences
        // never span it.
        flush(&mut pending, &mut doc);
        in_fence = false;
    }
    doc
}

/// A summary line plus one indented bullet per guidance sentence.
fn bulleted(summary: &str, bullets: &[String]) -> String {
    format!("{summary}\n  - {}", bullets.join("\n  - "))
}

/// Measures one markdown-prose region: fence tracking, indented-code and
/// exempt-content classification, and bullet segmentation over the stripped
/// lines.
///
/// `in_fence` carries the open-fence state in and out: a fence opened here
/// stays open until its closing fence line or the region's end.
fn measure_markdown_region(
    region: DocRegion,
    doc: &mut Document,
    pending: &mut Option<PendingParagraph>,
    in_fence: &mut bool,
) {
    for line in region.lines {
        let trimmed = line.text.trim();

        if trimmed.is_empty() {
            flush(pending, doc);
            doc.lines.push(StrippedLine {
                number: line.number,
                text: line.text,
                in_code_block: false,
            });
            continue;
        }

        // Decide whether this line is exempt from paragraph measuring.
        // A fence is a ``` or ~~~ line: it opens a code block, and the
        // next fence line closes it.
        //
        // Fence lines and everything between them are exempt. Outside a
        // block, indented code and lines like headings, tables, and
        // signature-like lines (full list on `is_exempt_content`) are also
        // exempt.
        let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        let in_code_block = *in_fence || fence || line.indented;
        let exempt = if *in_fence {
            if fence {
                *in_fence = false;
            }
            true
        } else if fence || line.indented || is_exempt_content(trimmed) {
            if fence {
                *in_fence = true;
            }
            true
        } else {
            false
        };

        // Count this line into the current paragraph (`pending`) or start
        // a new one. A paragraph is a run of consecutive doc lines, ended
        // by a blank line, an exempt line, or the start of a new bullet.
        if exempt {
            // Exempt lines are not paragraph text, so this is the end
            // of the current paragraph.
            flush(pending, doc);
        } else if let Some(content) = bullet_content(trimmed) {
            // A bullet ends the current paragraph and starts its own,
            // measured from the text after the bullet marker.
            flush(pending, doc);
            *pending = Some(PendingParagraph {
                kind: ParagraphKind::Bullet,
                first_line: line.number,
                len: content.chars().count(),
                count: 1,
            });
        } else if let Some(open) = pending.as_mut() {
            // Continuation lines (next plain line or wrapped bullet tail)
            // join the current paragraph; only trimmed text counts.
            open.len += trimmed.chars().count();
            open.count += 1;
        } else {
            // Plain text with no paragraph open: start one at this line.
            *pending = Some(PendingParagraph {
                kind: ParagraphKind::Plain,
                first_line: line.number,
                len: trimmed.chars().count(),
                count: 1,
            });
        }
        doc.lines.push(StrippedLine {
            number: line.number,
            text: line.text,
            in_code_block,
        });
    }
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

/// Folds the accumulated member lengths into a finished paragraph, if any.
fn flush(pending: &mut Option<PendingParagraph>, doc: &mut Document) {
    if let Some(open) = pending.take() {
        let joining_spaces = open.count.saturating_sub(1);
        doc.paragraphs.push(Paragraph {
            first_line: open.first_line,
            kind: open.kind,
            size: open.len + joining_spaces,
        });
    }
}

/// Whole-line exempt-content heuristics: headings, table rows, signature-like
/// lines, and link reference definitions. Exempt lines cost no paragraph
/// budget and end any open paragraph.
///
/// Code spans and URLs are not whole-line exemptions; those lines stay
/// paragraph members whose full text counts toward the budget.
fn is_exempt_content(trimmed: &str) -> bool {
    trimmed.starts_with('#')
        || trimmed.starts_with('|')
        || is_signature_line(trimmed)
        || is_link_reference_definition(trimmed)
}

/// True for markdown link reference definitions such as `[docs]: ./docs/x.md`.
fn is_link_reference_definition(trimmed: &str) -> bool {
    trimmed.starts_with('[') && trimmed.contains("]:")
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

    /// Returns only the diagnostics with the given code.
    ///
    /// Shared with the [`paragraph_length`] and [`line_length`] submodule
    /// tests.
    pub(crate) fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
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

    // ── Marker table: language independence ──

    // Markdown has no marker: every line is measured.
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

    // `cs`-style `//` comments strip through the same path as Rust.
    #[test]
    fn analyze_strips_cs_style_marker() {
        let source = indoc! {"
            // cs comment
            var x = 1;
        "};
        let doc = analyze(source, "cs");
        assert_eq!(doc.lines.len(), 1);
        assert_eq!(doc.lines[0].text, "cs comment");
    }

    // `py`-style `#` comments strip through the same path.
    #[test]
    fn analyze_strips_py_style_marker() {
        let source = indoc! {"
            # py comment
            x = 1
        "};
        let doc = analyze(source, "py");
        assert_eq!(doc.lines.len(), 1);
        assert_eq!(doc.lines[0].text, "py comment");
    }

    // ── Paragraph segmentation ──

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

    // ── Exemptions ──

    // Fenced code content is exempt: it forms no paragraph.
    #[test]
    fn analyze_exempts_fenced_code() {
        let source = indoc! {"
            text
            ```rust
            let x = 1;
            let y = 2;
            ```
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
            /// ```text
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
