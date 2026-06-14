//! Adapted from rust-reorder (MIT).
//! Modified based on <https://github.com/umwelt-ai/rust-reorder>.
//! Provides span extraction, comment pinning, preamble/trailer detection.
//!
//! This module orchestrates parsing: it splits source text into top-level
//! items with byte spans (comment-pinning prefix comments/attributes to each
//! item), classifies them, and exposes the data model. Public types are
//! re-exported from this module for downstream use.

use crate::parse::classify::classify_item;
pub use item::{ItemKind, ParseResult, SourceItem, VisibilityTier};
use syn::spanned::Spanned;

mod classify;
mod item;

/// Parse a Rust source file and extract items with spans.
///
/// Uses `syn` to parse the file and walks the AST to extract span ranges
/// for each top-level item. Comments and attributes that syntactically
/// precede an item are included in its span (comment-pinning).
///
/// # Errors
///
/// Returns a parse error when `source` is not valid Rust syntax
/// (the error is propagated from `syn::parse_str`).
pub fn parse_source(source: &str) -> anyhow::Result<ParseResult> {
    let file: syn::File = syn::parse_str(source)?;

    // Precompute the byte offset of the start of every line in a single pass.
    // syn reports spans as (1-based line, 0-based byte column); we look up the
    // line's start offset and add the column. Building this once turns each
    // line->offset conversion into an O(1) table lookup, so the whole span
    // extraction is O(source_len + items) instead of O(items * source_len).
    let line_starts = line_start_offsets(source);

    let mut item_spans: Vec<(usize, usize)> = Vec::new();
    for item in &file.items {
        let start = linecol_to_byte(
            &line_starts,
            item.span().start().line,
            item.span().start().column,
        );
        let end = linecol_to_byte(
            &line_starts,
            item.span().end().line,
            item.span().end().column,
        );
        item_spans.push((start, end));
    }

    if item_spans.is_empty() {
        return Ok(ParseResult {
            items: Vec::new(),
            source: source.to_string(),
            file,
            preamble_end: 0,
            trailer_start: source.len(),
        });
    }

    let preamble_end = item_spans[0].0;

    // Build items covering contiguous ranges.
    // Each item extends from its syn start to the syn start of the next item
    // (covering inter-item whitespace/comments). The last item extends to
    // include its trailing newline (before the trailer).
    let mut items = Vec::with_capacity(file.items.len());
    for i in 0..file.items.len() {
        let start = item_spans[i].0;
        let end = if i + 1 < file.items.len() {
            item_spans[i + 1].0
        } else {
            // Extend last item to end of its line (past trailing newline).
            // Look for the next \n after the item's syn end.
            let mut e = item_spans[i].1;
            if e < source.len() {
                let rest = &source[e..];
                if let Some(pos) = rest.find('\n') {
                    e += pos + 1; // include the \n
                } else {
                    e = source.len();
                }
            }
            e
        };

        // 1-based line of this item's start: count of line-start offsets that
        // are <= the item start byte (O(log lines) via binary search on the
        // sorted `line_starts` table).
        let start_line = line_starts.partition_point(|&s| s <= start);

        let class = classify_item(&file.items[i], source, start);
        items.push(SourceItem::new(
            start,
            end,
            start_line,
            class.kind,
            class.name,
            class.impl_target,
            class.is_test_module,
            class.is_trait_impl,
            class.visibility,
            class.doc_comments,
            class.returns_result,
            class.params,
            class.is_test_fn,
        ));
    }

    // Trailer: everything after the last item's extended end.
    let trailer_start = match items.last() {
        Some(last) => last.end,
        None => source.len(),
    };

    Ok(ParseResult {
        items,
        source: source.to_string(),
        file,
        preamble_end,
        trailer_start,
    })
}

/// Byte offset of the start of every line in `source`.
///
/// `starts[0]` is always `0` (line 1 starts at offset 0). Each subsequent
/// entry is the byte offset immediately following a `'\n'`, i.e. the first
/// byte of the next line. Built with a single SIMD-accelerated `memchr` scan.
fn line_start_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    // Heuristic preallocation. Capacity = bytes/D; no regrowth when the file's
    // average bytes/line >= D. Measured across 3820 Rust files (~1.25M lines):
    //   D=24 -> ~93% no regrow, D=21 -> ~96%, D=20 -> ~97%.
    // D=21 chosen for >95% target with margin (median file = ~33 bytes/line).
    let mut starts: Vec<usize> = Vec::with_capacity(bytes.len() / 21 + 1);
    starts.push(0);
    let mut from = 0;
    while let Some(pos) = memchr::memchr(b'\n', &bytes[from..]) {
        from += pos + 1;
        starts.push(from);
    }
    starts
}

/// Convert line/column (1-based line, 0-based byte column) to a byte offset
/// using the precomputed `line_starts` table for an O(1) lookup.
fn linecol_to_byte(line_starts: &[usize], line: usize, column: usize) -> usize {
    // syn line is 1-based; table index 0 holds line 1's start offset.
    let idx = line.saturating_sub(1);
    let base = line_starts.get(idx).copied().unwrap_or(usize::MAX);
    base + column
}
