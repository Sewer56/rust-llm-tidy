//! Plaintext extraction and segmentation for the paragraph and line-length
//! checks.
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
//! - [`Paragraph`] - a measured paragraph: plain prose or a bullet with its
//!   wrapped continuations.

// Currently unreferenced: the DOC007/DOC008 checks are the intended consumers.
#![allow(dead_code)]

/// Stripped lines and paragraphs extracted from one file.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Document {
    pub lines: Vec<StrippedLine>,
    pub paragraphs: Vec<Paragraph>,
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

/// Whether a paragraph is plain prose or a bullet with wrapped continuations.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParagraphKind {
    /// Consecutive prose lines up to a blank or exempt boundary line.
    Plain,
    /// A bullet marker line plus its wrapped continuation lines.
    Bullet,
}

/// Strips and segments `source` for the given file extension in one linear
/// pass. Lines without a matching comment marker are skipped entirely for
/// marker languages; for marker-less extensions every line is kept.
pub(crate) fn analyze(source: &str, ext: &str) -> Document {
    let markers = markers_for(ext);
    let mut doc = Document::default();
    // Open paragraph: kind, first member line, summed member length, member
    // count. `size` adds one joining space per extra member at flush time.
    let mut pending: Option<(ParagraphKind, usize, usize, usize)> = None;
    let mut in_fence = false;

    for (idx, raw) in source.split_inclusive('\n').enumerate() {
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);

        // One linear pass: strip, classify, and fold each line exactly once.
        let Some((text, raw_indent)) = strip(raw, markers) else {
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

        let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        // Marker languages keep code indentation before the marker, so only
        // post-marker indent counts there; marker-less files use raw indent.
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

        if exempt {
            flush(&mut pending, &mut doc);
        } else if let Some(content) = bullet_content(trimmed) {
            flush(&mut pending, &mut doc);
            pending = Some((ParagraphKind::Bullet, number, content.len(), 1));
        } else if let Some((_, _, len, count)) = pending.as_mut() {
            // Continuation lines join the open paragraph, bullet or plain;
            // marker and indent remnants are not paragraph text.
            *len += trimmed.len();
            *count += 1;
        } else {
            pending = Some((ParagraphKind::Plain, number, trimmed.len(), 1));
        }
    }
    flush(&mut pending, &mut doc);
    doc
}

/// Line-comment markers stripped before measurement, keyed by file extension,
/// longest marker first. Extensions outside the marker table use no marker, so
/// the whole file counts as prose.
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
fn flush(pending: &mut Option<(ParagraphKind, usize, usize, usize)>, doc: &mut Document) {
    if let Some((kind, first_line, len, count)) = pending.take() {
        let joining_spaces = count.saturating_sub(1);
        doc.paragraphs.push(Paragraph {
            first_line,
            kind,
            size: len + joining_spaces,
        });
    }
}

/// Conservative exempt-content heuristics: headings, table rows, URLs, code
/// spans, and signature-like lines. Exempt lines cost no paragraph budget.
fn is_exempt_content(trimmed: &str) -> bool {
    trimmed.starts_with('#')
        || trimmed.starts_with('|')
        || trimmed.contains("http://")
        || trimmed.contains("https://")
        || trimmed.contains('`')
        || is_signature_line(trimmed)
}

/// Strips leading whitespace, the first matching comment marker, and at most
/// one following space. Returns the stripped text plus the raw line's leading
/// whitespace count, or `None` when no marker matches in a marker language.
fn strip<'a>(raw: &'a str, markers: &[&str]) -> Option<(&'a str, usize)> {
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

/// True for lines that look like code signatures rather than prose.
fn is_signature_line(trimmed: &str) -> bool {
    for keyword in ["fn ", "struct ", "enum ", "trait ", "impl "] {
        if trimmed.contains(keyword) {
            return true;
        }
    }
    trimmed.ends_with(';')
        || trimmed.ends_with('{')
        || trimmed.ends_with('(')
        || trimmed.ends_with("->")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let doc = analyze("// a\n//! b\n/// c\n", "rs");
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
        let doc = analyze("let x = 1;\n// note\n", "rs");
        assert_eq!(doc.lines.len(), 1);
        assert_eq!(doc.lines[0].text, "note");
    }

    // ── Marker table: language independence ──

    // Markdown has no marker: the whole file is prose.
    #[test]
    fn analyze_keeps_all_markdown_lines() {
        let doc = analyze("# Title\n\nParagraph text.\n", "md");
        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.lines[0].text, "# Title");
    }

    // `cs`-style `//` comments strip through the same path as Rust.
    #[test]
    fn analyze_strips_cs_style_marker() {
        let doc = analyze("// cs comment\nvar x = 1;\n", "cs");
        assert_eq!(doc.lines.len(), 1);
        assert_eq!(doc.lines[0].text, "cs comment");
    }

    // `py`-style `#` comments strip through the same path.
    #[test]
    fn analyze_strips_py_style_marker() {
        let doc = analyze("# py comment\nx = 1\n", "py");
        assert_eq!(doc.lines.len(), 1);
        assert_eq!(doc.lines[0].text, "py comment");
    }

    // ── Paragraph segmentation ──

    // Blank lines split paragraphs; size joins lines with single spaces.
    #[test]
    fn analyze_splits_paragraphs_at_blank_lines() {
        let doc = analyze("/// one two\n/// three\n\n/// four\n", "rs");
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
        let source = "/// intro prose\n/// - bullet start\n///   wrapped tail\n";
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 2);
        let bullet = paragraph_at(&doc, 2).unwrap();
        assert_eq!(bullet.kind, ParagraphKind::Bullet);
        assert_eq!(bullet.size, "bullet start wrapped tail".len());
    }

    // Nested bullets are separate paragraphs, excluded from the parent bullet
    // and from any enclosing prose paragraph.
    #[test]
    fn analyze_separates_nested_bullets() {
        let source = "prose line\n- top bullet\n  - nested bullet\n";
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
        let doc = analyze("1. first\n2. second\n", "md");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().kind, ParagraphKind::Bullet);
    }

    // ── Exemptions ──

    // Fenced code content is exempt: it forms no paragraph.
    #[test]
    fn analyze_exempts_fenced_code() {
        let source = "text\n```rust\nlet x = 1;\nlet y = 2;\n```\nafter\n";
        let doc = analyze(source, "md");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "text".len());
        assert_eq!(paragraph_at(&doc, 6).unwrap().size, "after".len());
    }

    // Tab- or 4-space-indented doc lines are exempt indented code.
    #[test]
    fn analyze_exempts_indented_code() {
        let source = "/// prose\n///\n///     let x = 1;\n/// \tlet y = 2;\n";
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
        let source = "| a | b |\n# Heading\nsee https://example.com/x\nrun `cargo test`\n";
        let doc = analyze(source, "md");
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.lines.len(), 4);
    }

    // Signature-like lines are exempt content.
    #[test]
    fn analyze_exempts_signature_lines() {
        let source = "/// prose\n/// `fn do_thing(x: usize) -> bool;`\n";
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "prose".len());
    }

    // An exempt line is a paragraph boundary even between prose lines.
    #[test]
    fn analyze_breaks_paragraph_at_exempt_line() {
        let source = "/// first part\n/// see https://example.com\n/// second part\n";
        let doc = analyze(source, "rs");
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(paragraph_at(&doc, 1).unwrap().size, "first part".len());
        assert_eq!(paragraph_at(&doc, 3).unwrap().size, "second part".len());
    }

    // A mixed prose + code-span line is exempt as a whole: its prose
    // remainder costs no paragraph budget.
    #[test]
    fn analyze_exempts_whole_line_with_inline_code_span() {
        let doc = analyze("/// run `cargo test` to verify\n", "rs");
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.lines.len(), 1);
    }
}
