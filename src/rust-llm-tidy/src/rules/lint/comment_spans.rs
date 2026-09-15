//! Identify complete comments, excluding strings and doc attributes.

use crate::languages::backend_for;
use crate::source::ParseResult;
use core::ops::Range;

/// Prefer retained trees; otherwise parse only when comment boundaries are needed.
///
/// Returns `None` for unsupported or uncertain syntax. Each outer span includes
/// its nested comments, without duplicates.
pub(crate) fn comment_spans(
    source: &str,
    ext: &str,
    parsed: Option<&ParseResult>,
) -> Option<Vec<Range<usize>>> {
    let owned;
    let parsed = if let Some(parsed) = parsed {
        parsed
    } else if let Some(backend) = backend_for(ext) {
        owned = backend.parse(source).ok()?;
        &owned
    } else {
        return crate::text::comments::comment_spans(source, ext);
    };
    let root = parsed.syntax_tree().root_node();
    if root.has_error() {
        return None;
    }

    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if matches!(node.kind(), "comment" | "line_comment" | "block_comment") {
            ranges.push(node.byte_range());
        } else if cursor.goto_first_child() {
            continue;
        }

        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Some(ranges);
            }
        }
    }
}
