//! Plaintext extraction, segmentation, and the paragraph and line-length
//! checks built on them.
//!
//! Converts raw file text into numbered stripped doc lines, paragraphs, and
//! exemption classifications in one linear pass. Comment prefixes and indents
//! are stripped before measurement so any line-comment language works through
//! the marker table.
//!
//! # Layers
//!
//! - [`markers_for`] - data-driven comment markers keyed by file extension.
//! - [`analyze`] - strips, numbers, and segments the file.
//! - [`Paragraph`] - a measured paragraph: plain text or a bullet with its
//!   wrapped continuations.
//! - [`run_text_checks`] - DOC007/DOC008 over the analysis result, delegated
//!   to [`paragraph_length`] and [`line_length`].

use crate::diagnostic::Diagnostic;

mod line_length;
mod paragraph_length;

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

/// A measured paragraph. `size` is the length of the member lines joined with
/// single spaces; exempt lines are never members, so they cost nothing.
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
/// DOC007 fires an Error when a plain paragraph's measured size exceeds 240
/// chars, and a Warning when a bullet's does; DOC008 fires a Warning for every
/// stripped line over 80 chars, with no content exemptions.
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

/// Strips and segments `source` for the given file extension in one linear
/// pass. Lines without a matching comment marker are skipped entirely for
/// marker languages; for marker-less extensions every line is kept.
pub(crate) fn analyze(source: &str, ext: &str) -> Document {
    let markers = markers_for(ext);
    let mut doc = Document::default();
    let mut pending: Option<PendingParagraph> = None;
    let mut in_fence = false;

    for (idx, raw) in source.split_inclusive('\n').enumerate() {
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);

        // One linear pass: strip, classify, and fold each line exactly once.
        let Some((text, raw_indent)) = strip_comment_prefix(raw, markers) else {
            // A non-doc line breaks paragraph consecutiveness.
            flush(&mut pending, &mut doc);
            continue;
        };
        let number = idx + 1;
        let trimmed = text.trim();

        if trimmed.is_empty() {
            flush(&mut pending, &mut doc);
            doc.lines.push(StrippedLine {
                number,
                text: text.to_string(),
            });
            continue;
        }

        // Decide whether this line is exempt from paragraph measuring.
        // A fence is a ``` or ~~~ line: it opens a code block, and the
        // next fence line closes it.
        //
        // Fence lines and everything between them are exempt. Outside a
        // block, indented code (a tab or 4 spaces) and lines like
        // headings, tables, and URLs (full list on `is_exempt_content`)
        // are also exempt.
        let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        // Indented code: the stripped text starts with a tab or 4 spaces, or
        // the raw line was indented 4+ spaces in a marker-less file.
        let indented_code = text.starts_with('\t')
            || text.starts_with("    ")
            || (markers.is_empty() && raw_indent >= 4);
        let exempt = if in_fence {
            if fence {
                in_fence = false;
            }
            true
        } else if fence || indented_code || is_exempt_content(trimmed) {
            if fence {
                in_fence = true;
            }
            true
        } else {
            false
        };
        doc.lines.push(StrippedLine {
            number,
            text: text.to_string(),
        });

        // Count this line into the current paragraph (`pending`) or start
        // a new one. A paragraph is a run of consecutive doc lines, ended
        // by a blank line, an exempt line, or the start of a new bullet.
        if exempt {
            // Exempt lines are not paragraph text, so this is the end
            // of the current paragraph.
            flush(&mut pending, &mut doc);
        } else if let Some(content) = bullet_content(trimmed) {
            // A bullet ends the current paragraph and starts its own,
            // measured from the text after the bullet marker.
            flush(&mut pending, &mut doc);
            pending = Some(PendingParagraph {
                kind: ParagraphKind::Bullet,
                first_line: number,
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
            pending = Some(PendingParagraph {
                kind: ParagraphKind::Plain,
                first_line: number,
                len: trimmed.chars().count(),
                count: 1,
            });
        }
    }
    flush(&mut pending, &mut doc);
    doc
}

/// Line-comment markers stripped before measurement, keyed by file extension,
/// longest marker first. Extensions outside the marker table use no marker, so
/// the whole file counts as paragraph text.
pub(crate) fn markers_for(ext: &str) -> &'static [&'static str] {
    match ext {
        "rs" => &["///", "//!", "//"],
        "md" => &[],
        // Non-pipeline languages, proven by unit tests only.
        "cs" | "java" | "js" | "ts" => &["//"],
        "py" | "sh" => &["#"],
        _ => &[],
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

/// A summary line plus one indented bullet per guidance sentence.
fn bulleted(summary: &str, bullets: &[String]) -> String {
    format!("{summary}\n  - {}", bullets.join("\n  - "))
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

/// Conservative exempt-content heuristics: headings, table rows, URLs, code
/// spans, signature-like lines, and link reference definitions. Exempt lines
/// cost no paragraph budget.
fn is_exempt_content(trimmed: &str) -> bool {
    trimmed.starts_with('#')
        || trimmed.starts_with('|')
        || trimmed.contains("http://")
        || trimmed.contains("https://")
        || trimmed.contains('`')
        || is_signature_line(trimmed)
        || is_link_reference_definition(trimmed)
}

/// Strips leading whitespace, the first matching comment marker, and at most
/// one following space. Returns the stripped text plus the raw line's leading
/// whitespace count, or `None` when no marker matches in a marker language.
///
/// Marker languages (Rust markers shown):
///
/// ```text
/// raw line            -> stripped text       raw indent
/// "  /// let x = 1;"  -> "let x = 1;"        2
/// "//  space kept"    -> " space kept"       0
/// "let x = 1;"        -> None                -
/// ```
///
/// Without markers (Markdown), every line matches; only indent goes:
///
/// ```text
/// "    text"          -> "text"              4
/// ```
fn strip_comment_prefix<'a>(raw: &'a str, markers: &[&str]) -> Option<(&'a str, usize)> {
    let without_indent = raw.trim_start();
    let raw_indent = raw.len() - without_indent.len();
    if markers.is_empty() {
        return Some((without_indent, raw_indent));
    }
    for marker in markers {
        if let Some(after) = without_indent.strip_prefix(marker) {
            let after = after.strip_prefix(' ').unwrap_or(after);
            return Some((after, raw_indent));
        }
    }
    None
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

    // Table rows, headings, URLs, and code spans are exempt content.
    #[test]
    fn analyze_exempts_tables_headings_urls_code_spans() {
        let source = indoc! {"
            | a | b |
            # Heading
            see https://example.com/x
            run `cargo test`
        "};
        let doc = analyze(source, "md");
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.lines.len(), 4);
    }

    // Signature-like lines are exempt content.
    #[test]
    fn analyze_exempts_signature_lines() {
        let source = indoc! {"
            /// prose
            /// `fn do_thing(x: usize) -> bool;`
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "prose".len());
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

    // An exempt line is a paragraph boundary even between text lines.
    #[test]
    fn analyze_breaks_paragraph_at_exempt_line() {
        let source = indoc! {"
            /// first part
            /// see https://example.com
            /// second part
        "};
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "first part".len());
        assert_eq!(paragraph_at(&doc, 3).unwrap().size, "second part".len());
    }

    // A mixed text + code-span line is exempt as a whole: the rest of
    // the line costs no paragraph budget.
    #[test]
    fn analyze_exempts_whole_line_with_inline_code_span() {
        let doc = analyze("/// run `cargo test` to verify\n", "rs");
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.lines.len(), 1);
    }
}
