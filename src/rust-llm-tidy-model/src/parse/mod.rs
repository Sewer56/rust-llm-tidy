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
/// Uses `syn` to parse the file and walk the AST, extracting byte spans for
/// each top-level item. Comments and attributes that syntactically precede an
/// item are pinned to it (comment-pinning).
///
/// Spans are laid back-to-back so each item carries the blank lines and `//`
/// comments preceding it when reordered: each item's `end` is the byte after
/// its trailing newline, and every non-first item's `start` is the previous
/// item's `end`. The span mechanics live in `build_items`.
///
/// # Errors
///
/// Returns a parse error when `source` is not valid Rust syntax
/// (propagated from `syn::parse_str`).
pub fn parse_source(source: &str) -> anyhow::Result<ParseResult> {
    let file: syn::File = syn::parse_str(source)?;

    // Byte offset of the start of every line. Built once so each syn
    // line->offset conversion is an O(1) lookup, making the whole span
    // extraction O(source_len + items) instead of O(items * source_len).
    let line_starts = line_start_offsets(source);
    let item_spans = syn_item_spans(&file, &line_starts);

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
    let items = build_items(&file.items, source, &line_starts, &item_spans, preamble_end);

    // Trailer: everything after the last item's extended end.
    let trailer_start = items.last().map_or(source.len(), |last| last.end);

    Ok(ParseResult {
        items,
        source: source.to_string(),
        file,
        preamble_end,
        trailer_start,
    })
}

/// Assign each item a span that carries the blank lines and `//` comments
/// preceding it, so reordering preserves that whitespace.
///
/// Spans are laid back-to-back: each item's `start` is the previous item's
/// `end` (`preamble_end` for the first), and each `end` is the byte after the
/// item's trailing newline. Consecutive spans thus touch with no overlap, and
/// the gap between two items' bodies falls inside the second item's span. For:
///
/// ```text
/// fn a() {}
///
/// // header
/// fn b() {}
/// ```
///
/// item `a` ends right after its own newline; item `b` starts there, so
/// `source[b.start..b.end]` contains the blank line and `// header`.
///
/// `start_line` and classification use the SYN start, not the trivia-extended
/// `start`, so diagnostic line numbers stay on the real body.
///
/// One advancing cursor over `line_starts` computes every `start_line` and
/// `end`; syn_start/syn_end are monotonic (items are in source order), so this
/// is a single O(items + lines) sweep, not O(items * log lines) binary searches.
///
/// # Arguments
///
/// * `items` - syn items in source order; indexed in parallel with `item_spans`.
/// * `source` - the full source text; classified names and spans index into it.
/// * `line_starts` - byte offset of the start of each line (from
///   `line_start_offsets`); sorted ascending, `starts[0] == 0`.
/// * `item_spans` - per-item `(syn_start, syn_end)` byte offsets, parallel to
///   `items`; each tuple's start/end are the SYN body boundaries (not yet
///   newline-extended).
/// * `preamble_end` - byte offset where the first item's span begins (end of
///   the leading file doc/attrs); seeds the back-to-back chain.
fn build_items(
    items: &[syn::Item],
    source: &str,
    line_starts: &[usize],
    item_spans: &[(usize, usize)],
    preamble_end: usize,
) -> Vec<SourceItem> {
    let source_len = source.len();
    let mut out = Vec::with_capacity(items.len());
    // prev_end seeds the first item's start with preamble_end, so no special
    // case is needed for it.
    let mut prev_end = preamble_end;
    let mut line_cursor: usize = 0;

    for (item, &(syn_start, syn_end)) in items.iter().zip(item_spans) {
        let start = prev_end;

        // 1-based start_line: count of line-start offsets at or before
        // syn_start (not the gap-extended start).
        while line_cursor < line_starts.len() && line_starts[line_cursor] <= syn_start {
            line_cursor += 1;
        }
        let start_line = line_cursor;

        // `end` extends syn_end to the byte after its terminating newline.
        while line_cursor < line_starts.len() && line_starts[line_cursor] <= syn_end {
            line_cursor += 1;
        }
        let end = line_starts.get(line_cursor).copied().unwrap_or(source_len);
        prev_end = end;

        let class = classify_item(item, source, syn_start);
        out.push(SourceItem::new(
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
    out
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

/// Byte spans of each top-level item, converted from syn's (1-based line,
/// 0-based byte column) coordinates via the `line_starts` table.
///
/// # Arguments
///
/// * `file` - parsed source; its `items` are walked in source order.
/// * `line_starts` - byte offset of the start of each line; used to turn
///   syn's (line, column) into a flat byte offset via `linecol_to_byte`.
///
/// Returns `(syn_start_byte, syn_end_byte)` per item - the SYN body
/// boundaries, before any newline/trivia extension.
fn syn_item_spans(file: &syn::File, line_starts: &[usize]) -> Vec<(usize, usize)> {
    file.items
        .iter()
        .map(|item| {
            let span = item.span();
            let start = linecol_to_byte(line_starts, span.start().line, span.start().column);
            let end = linecol_to_byte(line_starts, span.end().line, span.end().column);
            (start, end)
        })
        .collect()
}

/// Convert line/column (1-based line, 0-based byte column) to a byte offset
/// using the precomputed `line_starts` table for an O(1) lookup.
fn linecol_to_byte(line_starts: &[usize], line: usize, column: usize) -> usize {
    // syn line is 1-based; table index 0 holds line 1's start offset.
    let idx = line.saturating_sub(1);
    let base = line_starts.get(idx).copied().unwrap_or(usize::MAX);
    base + column
}

#[cfg(test)]
mod tests {
    use super::parse_source;

    /// Gap-anchored spans: each non-first item's `start` is the previous
    /// item's `end`, `end` includes the trailing newline, and `start_line`
    /// tracks the SYN start (not the gap-extended start).
    #[test]
    fn gap_anchored_spans_and_start_lines() {
        proc_macro2::fallback::force();
        // Line map (1-based):
        //   1: //! doc
        //   2: fn b() {}
        //   3: (blank)
        //   4: // section header
        //   5: (blank)
        //   6: fn a() {}
        let source = "\
//! doc\n\
fn b() {}\n\
\n\
// section header\n\
\n\
fn a() {}\n";

        let parsed = parse_source(source).unwrap();

        // Two top-level items: b (line 2), a (line 6).
        assert_eq!(parsed.items.len(), 2, "two top-level items");

        // start_line uses the SYN start, not the gap-extended start.
        assert_eq!(parsed.items[0].start_line(), 2, "item 0 on line 2");
        assert_eq!(parsed.items[1].start_line(), 6, "item 1 on line 6");

        // Gap-anchoring: item 1's start == item 0's end.
        assert_eq!(
            parsed.items[1].start, parsed.items[0].end,
            "item 1 start == item 0 end (gap-anchored)"
        );

        // Each item's end includes its trailing newline.
        let item0_body = &source[parsed.items[0].start..parsed.items[0].end];
        assert!(
            item0_body.ends_with('\n'),
            "item 0 end includes trailing \\n"
        );
        let item1_body = &source[parsed.items[1].start..parsed.items[1].end];
        assert!(
            item1_body.ends_with('\n'),
            "item 1 end includes trailing \\n"
        );

        // Item 1's slice carries the inter-item gap as leading trivia.
        assert!(
            item1_body.starts_with('\n'),
            "item 1 slice starts with carried gap (the \\n ending item 0's line)"
        );
        assert!(
            item1_body.contains("// section header"),
            "item 1 slice carries the // section header"
        );

        // Preamble is the module doc line.
        assert_eq!(&source[..parsed.preamble_end], "//! doc\n");

        // Trailer is empty (source ends right after the last item's newline).
        assert_eq!(parsed.trailer_start, source.len());
    }

    /// Last item without a trailing newline: `end` extends to `source.len()`.
    #[test]
    fn last_item_no_trailing_newline() {
        proc_macro2::fallback::force();
        let source = "fn a() {}\nfn b() {}"; // no final \n
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(
            parsed.items[1].end,
            source.len(),
            "last item end == source.len() when no trailing newline"
        );
        // Trailer is empty.
        assert_eq!(parsed.trailer_start, source.len());
    }

    /// Single item: start == preamble_end, end extends through trailing newline.
    #[test]
    fn single_item() {
        proc_macro2::fallback::force();
        let source = "//! doc\nfn main() {}\n";
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].start_line(), 2);
        assert_eq!(parsed.items[0].start, parsed.preamble_end);
        let body = &source[parsed.items[0].start..parsed.items[0].end];
        assert!(body.starts_with("fn main"));
        assert!(body.ends_with('\n'));
    }
}
