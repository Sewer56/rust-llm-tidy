// Partially vendored from rust-reorder (MIT).
// Modified based on https://github.com/umwelt-ai/rust-reorder.
// Provides permutation validation and byte-slice emit.

use anyhow::{Result, ensure};
use rust_llm_tidy_model::line_endings::dominant_line_ending;
use rust_llm_tidy_model::parse::{ItemKind, ParseResult};

/// A validated permutation of items.
///
/// Wraps a `Vec<usize>` that maps output position → input item index.
/// Every index in `0..n` appears exactly once.
#[derive(Debug, Clone)]
pub struct Permutation {
    order: Vec<usize>,
}

impl Permutation {
    /// Create a new permutation.
    ///
    /// `n` is the total number of items.
    /// `order` must contain every index in `0..n` exactly once.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `order.len() != n` (length mismatch).
    /// - any index in `order` is `>= n` (out of range).
    /// - any index appears more than once in `order` (duplicate).
    pub fn new(n: usize, order: Vec<usize>) -> Result<Self> {
        ensure!(
            order.len() == n,
            "permutation length {} does not match item count {}",
            order.len(),
            n
        );

        let mut seen = vec![false; n];
        for &idx in &order {
            ensure!(idx < n, "permutation index {} out of range (n={})", idx, n);
            ensure!(!seen[idx], "duplicate index {} in permutation", idx);
            seen[idx] = true;
        }

        Ok(Self { order })
    }

    /// Return the underlying order vector.
    pub fn into_inner(self) -> Vec<usize> {
        self.order
    }
}

/// Emit the reordered source by byte-slicing the original source.
///
/// Extracts each item's byte range from `parsed.source` and concatenates
/// them in the permutation order. Because items are gap-anchored to the
/// next item, a slice may begin with carried leading trivia (blank lines
/// and plain `//` section headers). Leading and trailing whitespace are
/// stripped ([`str::trim`]) so separators do not pile up when items move,
/// while the `//` header and `///`/`//!` doc lines (non-whitespace) are
/// preserved. Inter-item spacing is then re-derived from the compact-group
/// logic below. The preamble and trailer are placed at the start and end.
///
/// # Arguments
///
/// - `parsed` - the parsed Rust source being reordered; its `source`, item
///   byte spans, `preamble_end`, and `trailer_start` drive the output.
/// - `perm` - the validated [`Permutation`] mapping output position to input
///   item index.
///
/// # Errors
///
/// Returns an [`anyhow::Error`] if the permutation is malformed for the
/// parsed items (e.g. an index out of range, which surfaces as a
/// slice-out-of-bounds failure).
///
/// # Line endings
///
/// Item-terminator and blank-line separators use the source's dominant
/// line ending ([`dominant_line_ending`]), so an in-place reorder never
/// flips CRLF <-> LF.
pub fn emit(parsed: &ParseResult, perm: &Permutation) -> Result<String> {
    let source = &parsed.source;
    let le = dominant_line_ending(source);
    let mut output = String::with_capacity(source.len());

    // Preamble: everything before the first item's start.
    output.push_str(&source[..parsed.preamble_end]);

    // Emit items in permutation order with canonical spacing:
    // - no blank line between consecutive `use` items
    // - no blank line between consecutive `mod` items
    // - no blank line between consecutive `const`/`static`/`extern` items
    // - blank line between everything else
    for (i, &idx) in perm.order.iter().enumerate() {
        let item = &parsed.items[idx];
        let slice = &source[item.start..item.end];
        output.push_str(slice.trim());
        output.push_str(le);

        if i + 1 < perm.order.len() {
            let next = &parsed.items[perm.order[i + 1]];
            let same_compact_group = matches!(
                (spacing_group(item.kind()), spacing_group(next.kind())),
                (Some(a), Some(b)) if a == b
            );
            if !same_compact_group {
                output.push_str(le);
            }
        }
    }

    // Trailer: everything after the last item's end.
    if parsed.trailer_start < source.len() {
        output.push_str(&source[parsed.trailer_start..]);
    }

    Ok(output)
}

/// Spacing group for items that should stay packed without a blank line.
///
/// Returns `None` for all other items, which always get a blank line after them.
fn spacing_group(kind: &ItemKind) -> Option<u8> {
    match kind {
        ItemKind::Use => Some(0),
        ItemKind::Mod => Some(1),
        ItemKind::Const | ItemKind::Static | ItemKind::Extern => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::compute_order;
    use rust_llm_tidy_model::parse::parse_source;

    /// Full reorder pipeline: parse, compute order, build permutation, emit.
    fn reorder(source: &str) -> String {
        let parsed = parse_source(source).unwrap();
        let order = compute_order(&parsed).unwrap();
        let perm = Permutation::new(parsed.items.len(), order).unwrap();
        emit(&parsed, &perm).unwrap()
    }

    #[test]
    fn emit_preserves_crlf_separators() {
        // CRLF source: every `\n` in the emitted output must be part of `\r\n`.
        let src = "fn b() { a(); }\r\nfn a() {}\r\n";
        let out = reorder(src);
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "every newline must be CRLF after reorder: {out:?}"
        );
        // Caller (b) before callee (a).
        let b = out.find("fn b").unwrap();
        let a = out.find("fn a").unwrap();
        assert!(b < a, "b (caller) before a (callee)");
    }

    #[test]
    fn emit_preserves_lf_separators() {
        // LF source: no `\r` should appear in the emitted output.
        let src = "fn b() { a(); }\nfn a() {}\n";
        let out = reorder(src);
        assert!(!out.contains('\r'), "no CR in LF output: {out:?}");
        let b = out.find("fn b").unwrap();
        let a = out.find("fn a").unwrap();
        assert!(b < a, "b (caller) before a (callee)");
    }

    #[test]
    fn emit_crlf_preserves_doc_comment_endings() {
        // CRLF source with a doc-comment-pinned item: the doc lines keep their
        // `\r\n` (verbatim byte slices) and the separators use `\r\n`.
        let src = "fn b() { a(); }\r\n/// docs for a\r\nfn a() {}\r\n";
        let out = reorder(src);
        assert!(
            out.contains("/// docs for a\r\n"),
            "doc-comment line ending preserved: {out:?}"
        );
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "every newline must be CRLF: {out:?}"
        );
    }
}
