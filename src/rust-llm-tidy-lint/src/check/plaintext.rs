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
//! - [`Paragraph`] - a measured paragraph: plain prose or a bullet with its
//!   wrapped continuations.
//! - [`run_text_checks`] - DOC007/DOC008 over the analysis result.

use crate::check::{CODE_LINE_LENGTH, CODE_PARAGRAPH_SIZE};
use crate::diagnostic::{Diagnostic, Severity};

/// Recommended maximum bullet length, stated in the shortening guidance.
const BULLET_RECOMMENDED: usize = 160;
/// Maximum stripped line length before DOC008 fires.
const LINE_LIMIT: usize = 80;
/// Maximum measured paragraph size before DOC007 fires.
const PARAGRAPH_LIMIT: usize = 240;

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
/// - `path` - the file path, named in each message.
///
/// # Returns
///
/// Diagnostics in source order: DOC007 per over-limit paragraph (bullet
/// warnings after their paragraph position), then DOC008 per over-limit line.
pub fn run_text_checks(source: &str, ext: &str, path: &str) -> Vec<Diagnostic> {
    let doc = analyze(source, ext);

    let mut diags = Vec::new();
    for para in &doc.paragraphs {
        if para.size <= PARAGRAPH_LIMIT {
            continue;
        }
        diags.push(match para.kind {
            ParagraphKind::Plain => paragraph_diagnostic(path, para),
            ParagraphKind::Bullet => bullet_diagnostic(path, para),
        });
    }
    for line in &doc.lines {
        let len = line.text.chars().count();
        if len > LINE_LIMIT {
            diags.push(line_length_diagnostic(path, line, len));
        }
    }
    diags
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

/// DOC007 Warning for an over-limit bullet, with shortening guidance.
fn bullet_diagnostic(path: &str, para: &Paragraph) -> Diagnostic {
    let guidance = format!(
        "{path}: bullet at line {} measures {} chars, over the \
         {PARAGRAPH_LIMIT}-char limit. Shorten it to one checkable action, \
         <= {BULLET_RECOMMENDED} chars recommended; split it at the nearest \
         idea change with a blank line or into separate bullets.",
        para.first_line, para.size
    );
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_PARAGRAPH_SIZE,
        message: guidance,
        line: para.first_line,
        item_kind: "file".to_string(),
        item_name: None,
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

/// DOC008 Warning for one over-limit stripped line.
fn line_length_diagnostic(path: &str, line: &StrippedLine, len: usize) -> Diagnostic {
    let guidance = format!(
        "{path}: line {} is {len} chars long, over the {LINE_LIMIT}-char \
         limit. Split it at the nearest idea change with a blank line; \
         code-block lines count too.",
        line.number
    );
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_LINE_LENGTH,
        message: guidance,
        line: line.number,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// DOC007 Error for an over-limit plain paragraph, reported at its first line.
fn paragraph_diagnostic(path: &str, para: &Paragraph) -> Diagnostic {
    let guidance = format!(
        "{path}: paragraph at line {} measures {} chars, over the {PARAGRAPH_LIMIT}-char \
         limit. Split it at the nearest idea change with a blank line; convert \
         list-like paragraphs into bullets (one checkable action each, \
         <= {BULLET_RECOMMENDED} chars); move remarks into their own sections. \
         Code, URLs, tables, headings, and signature lines are exempt: do not \
         split them.",
        para.first_line, para.size
    );
    Diagnostic {
        severity: Severity::Error,
        code: CODE_PARAGRAPH_SIZE,
        message: guidance,
        line: para.first_line,
        item_kind: "file".to_string(),
        item_name: None,
    }
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

    // ── DOC007: paragraph and bullet budgets ──

    // Returns only the diagnostics with the given code.
    fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    // Builds a `///` comment paragraph of `words` filler words.
    fn paragraph_source(words: usize) -> String {
        (0..words)
            .map(|i| format!("/// w{i}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    // Over-limit plain paragraph -> DOC007 Error at the first line, with
    // split and exemption guidance.
    #[test]
    fn text_checks_error_on_oversized_plain_paragraph() {
        let source = paragraph_source(80);
        let diags = run_text_checks(&source, "rs", "file.rs");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.contains("file.rs"), "message must name the file");
        assert!(msg.contains("over the 240-char limit"));
        assert!(msg.contains("blank line"));
        assert!(msg.contains("bullets"));
        assert!(msg.contains("160"));
        assert!(msg.contains("URLs, tables, headings, and signature"));
    }

    // Paragraph at or under the limit is silent.
    #[test]
    fn text_checks_silent_on_paragraph_within_limit() {
        let source = paragraph_source(30);
        let diags = run_text_checks(&source, "rs", "file.rs");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // A paragraph measuring exactly 240 chars is at the limit, not over it.
    #[test]
    fn text_checks_silent_on_paragraph_at_exact_limit() {
        let source = format!("/// {}\n", "x".repeat(240));
        let diags = run_text_checks(&source, "rs", "file.rs");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // The Error is reported at the paragraph's first line, not where the
    // budget overflowed.
    #[test]
    fn text_checks_report_paragraph_at_its_first_line() {
        let source = format!(
            "/// short intro\n\n{}\n",
            "/// ".to_string() + &"y".repeat(300)
        );
        let diags = run_text_checks(&source, "rs", "file.rs");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    // Over-limit bullet -> DOC007 Warning only, with shortening guidance and
    // the 160-char recommendation.
    #[test]
    fn text_checks_warn_on_oversized_bullet() {
        let bullet = "- ".to_string() + &"word ".repeat(60);
        let source = format!("{bullet}\n");
        let diags = run_text_checks(&source, "md", "file.md");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.contains("Shorten"));
        assert!(msg.contains("160"));
        assert!(msg.contains("bullets"));
    }

    // Bullet within the limit is silent.
    #[test]
    fn text_checks_silent_on_bullet_within_limit() {
        let source = format!("- {}\n", "word ".repeat(10));
        let diags = run_text_checks(&source, "md", "file.md");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // Bullet content never inflates the enclosing paragraph's budget.
    #[test]
    fn text_checks_exclude_bullets_from_paragraph_budget() {
        let prose = "sentence ".repeat(20);
        let bullet = "- ".to_string() + &"word ".repeat(20);
        // Prose alone stays under 240; adding the bullet words would cross it.
        let source = format!("{prose}\n{bullet}\n");
        assert!(prose.trim().len() < 240);
        let diags = run_text_checks(&source, "md", "file.md");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // A whole-line code-span exemption keeps its prose remainder free of the
    // paragraph budget.
    #[test]
    fn text_checks_exempt_line_costs_no_paragraph_budget() {
        let prose = "sentence ".repeat(25);
        let source = format!("{prose}\nrun `cargo test` now please\n");
        let diags = run_text_checks(&source, "md", "file.md");
        assert!(codes(&diags, CODE_PARAGRAPH_SIZE).is_empty());
    }

    // ── DOC008: line length ──

    // Over-limit stripped line -> DOC008 Warning naming file, line, measured
    // length, and the limit. Indent and comment marker are not measured.
    #[test]
    fn text_checks_warn_on_long_stripped_line() {
        let text = "x".repeat(81);
        let source = format!("\t/// {text}\n");
        let diags = run_text_checks(&source, "rs", "file.rs");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.contains("file.rs"));
        assert!(msg.contains("line 1"));
        assert!(msg.contains("81 chars"));
        assert!(msg.contains("80-char limit"));
    }

    // A line of exactly 80 chars passes.
    #[test]
    fn text_checks_silent_on_line_at_limit() {
        let source = format!("{}\n", "x".repeat(80));
        let diags = run_text_checks(&source, "md", "file.md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // No content exemptions: fenced code-block lines also warn.
    #[test]
    fn text_checks_warn_on_long_lines_inside_fenced_code() {
        let inner = "y".repeat(90);
        let source = format!("```\n{inner}\n```\n");
        let diags = run_text_checks(&source, "md", "file.md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
    }
}
