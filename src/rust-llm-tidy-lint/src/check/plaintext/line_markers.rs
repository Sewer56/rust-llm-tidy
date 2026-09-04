//! The legacy doc-region producer: line-comment markers.
//!
//! Each file splits into one region per contiguous run of comment lines;
//! marker-less extensions (the markdown family) produce one whole-file
//! region.
//!
//! Non-comment lines emit nothing and end the current region, so the
//! measuring core breaks paragraphs and fences at the gap.
//!
//! The Rust AST producer merges this producer's `rs` regions with the
//! parse tree's block and attribute doc regions.

use super::region::{Dialect, DocRegion, RegionLine};

/// Produces the file's doc regions: one region per contiguous run of
/// comment lines, in source order.
///
/// Marker-less extensions yield a single region holding every line.
/// The Rust doc-region producer also consumes these regions, merging
/// them with its parse-tree doc regions.
///
/// # Arguments
///
/// - `source` - the raw file text.
/// - `ext` - the file extension, selecting the comment marker table.
pub fn doc_regions(source: &str, ext: &str) -> Vec<DocRegion> {
    let markers = markers_for(ext);
    let mut regions: Vec<DocRegion> = Vec::new();
    let mut in_region = false;
    for (idx, raw) in source.split_inclusive('\n').enumerate() {
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let Some((text, raw_indent)) = strip_comment_prefix(raw, markers) else {
            // A non-comment line emits nothing and ends the current region.
            in_region = false;
            continue;
        };
        // Indented code: a tab or 4-space lead after the marker. In
        // marker-less files the strip removed all leading whitespace, so
        // the raw indent decides.
        let indented = if markers.is_empty() {
            raw_indent >= 4
        } else {
            text.starts_with('\t') || text.starts_with("    ")
        };
        let line = RegionLine {
            number: idx + 1,
            text: text.to_string(),
            indented,
        };
        if in_region {
            // `in_region` is true only after a region was pushed, so the
            // expect cannot fire.
            regions.last_mut().expect("open region").lines.push(line);
        } else {
            regions.push(DocRegion {
                dialect: Dialect::Markdown,
                lines: vec![line],
            });
            in_region = true;
        }
    }
    regions
}

/// Line-comment markers stripped before measurement, keyed by file extension,
/// longest marker first. Extensions outside the marker table use no marker, so
/// the whole file counts as paragraph text.
fn markers_for(ext: &str) -> &'static [&'static str] {
    match ext {
        "rs" => &["///", "//!", "//"],
        "md" => &[],
        // Rows kept for this producer's own unit tests; pipeline dispatch
        // routes `cs` text checks through its AST doc-region producer and
        // runs no other row's tier yet.
        "cs" | "java" | "js" | "ts" => &["//"],
        "py" | "sh" => &["#"],
        _ => &[],
    }
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
