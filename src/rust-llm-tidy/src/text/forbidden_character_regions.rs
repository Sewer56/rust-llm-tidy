//! Classify TEXT009 prose as documentation or ordinary comments.

use crate::languages::{python, rust};
use crate::source::ParseResult;
use crate::text::comment_spans::comment_spans;
use crate::text::measurement::{Dialect, DocRegion, RegionLine};

/// Extract parsed comments and recognized string-based documentation separately.
/// Syntax-error trees yield no regions, matching shared comment recognition.
pub(crate) fn parsed_regions(parsed: &ParseResult, ext: &str) -> (Vec<DocRegion>, Vec<DocRegion>) {
    let source = parsed.source.as_str();
    let Some(spans) = comment_spans(source, ext, Some(parsed)) else {
        return (Vec::new(), Vec::new());
    };
    let mut docs = match ext.to_ascii_lowercase().as_str() {
        "rs" => rust::text_regions::attribute_regions(parsed),
        "py" | "pyi" => python::text_regions::doc_regions(parsed)
            .into_iter()
            .filter(|region| region.dialect == Dialect::Docstring)
            .collect(),
        _ => Vec::new(),
    };
    let mut comments = Vec::new();
    let mut previous_end = 0;
    let mut row = 1;
    let mut previous_line_comment = false;
    let mut previous_docs = false;
    let mut line_start = 0;

    for span in spans {
        if let Some(newline) = source[previous_end..span.start].rfind('\n') {
            line_start = previous_end + newline + 1;
        }
        row += source[previous_end..span.start]
            .bytes()
            .filter(|&b| b == b'\n')
            .count();
        let raw = &source[span.clone()];
        let block = raw.starts_with("/*");
        let is_docs = (ext.eq_ignore_ascii_case("rs")
            && (raw.starts_with("//!") || raw.starts_with("/*!")))
            || ((ext.eq_ignore_ascii_case("rs") || ext.eq_ignore_ascii_case("cs"))
                && ((raw.starts_with("///") && !raw.starts_with("////"))
                    || (raw.starts_with("/**") && !raw.starts_with("/***"))));
        let dialect = if is_docs && ext.eq_ignore_ascii_case("cs") {
            Dialect::XmlDoc
        } else if block {
            Dialect::BlockDoc
        } else {
            Dialect::Markdown
        };
        let body = if block {
            let body = raw.strip_prefix("/*").unwrap_or(raw);
            let body = if is_docs { &body[1..] } else { body };
            body.strip_suffix("*/").unwrap_or(body)
        } else {
            let body = raw.trim_start_matches(['/', '#']);
            if is_docs {
                body.strip_prefix('!').unwrap_or(body)
            } else {
                body
            }
        };
        let lines = comment_lines(body, row, block);

        let target = if is_docs { &mut docs } else { &mut comments };
        let standalone = source[line_start..span.start].trim().is_empty();
        let continues = !block
            && standalone
            && previous_line_comment
            && previous_docs == is_docs
            && source[previous_end..span.start].trim().is_empty()
            && target.last().is_some_and(|region| {
                region.dialect == dialect
                    && region
                        .lines
                        .last()
                        .is_some_and(|line| line.number + 1 == row)
            });
        if continues {
            target
                .last_mut()
                .expect("continuation has a region")
                .lines
                .extend(lines);
        } else {
            target.push(DocRegion { dialect, lines });
        }

        row += raw.bytes().filter(|&b| b == b'\n').count();
        if let Some(newline) = raw.rfind('\n') {
            line_start = span.start + newline + 1;
        }
        previous_end = span.end;
        previous_line_comment = !block && standalone;
        previous_docs = is_docs;
    }

    docs.sort_by_key(|region| region.lines.first().map_or(0, |line| line.number));
    (docs, comments)
}

/// Remove comment indentation while preserving relative example indentation.
fn comment_lines(body: &str, row: usize, block: bool) -> Vec<RegionLine> {
    let continuation_indent = if block {
        body.lines()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
            .min()
            .unwrap_or(0)
    } else {
        0
    };

    body.lines()
        .enumerate()
        .map(|(offset, text)| {
            // Source indentation precedes the marker; code indentation
            // after it must reach the block-doc dialect unchanged.
            let text = if block && text.trim_start().starts_with('*') {
                text.trim_start()
            } else if block && offset > 0 {
                &text[continuation_indent.min(text.len())..]
            } else {
                text.strip_prefix(' ').unwrap_or(text)
            };

            RegionLine {
                number: row + offset,
                text: text.to_owned(),
                indented: !block && (text.starts_with('\t') || text.starts_with("    ")),
            }
        })
        .collect()
}
