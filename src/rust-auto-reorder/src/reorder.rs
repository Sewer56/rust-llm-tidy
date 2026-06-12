// Partially vendored from rust-reorder (MIT).
// Modified based on https://github.com/umwelt-ai/rust-reorder.
// Provides permutation validation and byte-slice emit.

use crate::parse::{ItemKind, ParseResult};
use anyhow::{Result, ensure};

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
    /// `order` is a sequence of indices that should cover each index 0..n exactly once.
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
/// them in the permutation order. Items already include inter-item
/// whitespace (including trailing newlines). The preamble and trailer
/// are placed at the start and end.
pub fn emit(parsed: &ParseResult, perm: &Permutation) -> Result<String> {
    let source = &parsed.source;
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
        output.push_str(slice.trim_end());
        output.push('\n');

        if i + 1 < perm.order.len() {
            let next = &parsed.items[perm.order[i + 1]];
            let same_compact_group = matches!(
                (spacing_group(item.kind()), spacing_group(next.kind())),
                (Some(a), Some(b)) if a == b
            );
            if !same_compact_group {
                output.push('\n');
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
