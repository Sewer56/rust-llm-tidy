//! Authorize text fixes only within parser-owned standalone line-comment runs.

use crate::languages::backend_for;
use crate::rules::transform::tables::strip_comment_prefix;
use core::ops::Range;

/// A contiguous comment run with one indentation and marker, in source bytes.
pub(super) struct CommentRun {
    pub bytes: Range<usize>,
    /// Zero-based source row for translating transform-local change anchors.
    pub row: usize,
}

/// Collect rewrite boundaries without copying the source into the item model.
///
/// Unsupported grammars, parse failures, and syntax-error trees authorize no
/// edits. Block comments, inline comments, and documentation literals are not
/// rewrite boundaries. Marker or indentation changes start a separate run.
///
/// Runs are returned in ascending source order and never overlap; only
/// touching ranges with the same prefix merge into one run.
/// Runs overlapping any protected byte range are omitted in full.
pub(super) fn comment_runs_protected(
    source: &str,
    ext: &str,
    prefixes: &[&str],
    ranges: &[Range<usize>],
) -> Vec<CommentRun> {
    // Only a complete, error-free parse can authorize source edits.
    let Some(language) = backend_for(ext).and_then(|backend| backend.language().ok()) else {
        return Vec::new();
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    if root.has_error() {
        return Vec::new();
    }

    let mut runs: Vec<CommentRun> = Vec::new();
    let mut previous_prefix = "";
    let mut cursor = root.walk();

    // Visit nodes in source order; markers inside literals are not comment nodes.
    'walk: loop {
        let node = cursor.node();
        if matches!(node.kind(), "line_comment" | "comment")
            && let Some((bytes, prefix)) = standalone_line(node, source, prefixes)
        {
            // Join only touching full lines with the same indentation and comment marker.
            if let Some(last) = runs.last_mut()
                && last.bytes.end == bytes.start
                && previous_prefix == prefix
            {
                last.bytes.end = bytes.end;
            } else {
                runs.push(CommentRun {
                    bytes,
                    row: node.start_position().row,
                });
            }
            previous_prefix = prefix;
        }

        // Descend first, then climb until a sibling remains or the tree is exhausted.
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() {
                // Drop the whole run: partial fixes could change fence/link ownership.
                runs.retain(|run| {
                    !ranges
                        .iter()
                        .any(|range| run.bytes.start < range.end && range.start < run.bytes.end)
                });
                return runs;
            }
        }
    }
}

/// Include indentation and the line ending only when the node owns a full line.
fn standalone_line<'a>(
    node: tree_sitter::Node<'_>,
    source: &'a str,
    prefixes: &[&str],
) -> Option<(Range<usize>, &'a str)> {
    let start = node.start_byte() - node.start_position().column;
    if !source[start..node.start_byte()]
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t'))
    {
        return None;
    }

    // Rust doc-comment nodes can include their newline; other comments do not.
    let content_end = source[start..node.end_byte()]
        .trim_end_matches(['\r', '\n'])
        .len()
        + start;
    let line = &source[start..content_end];
    let (prefix, body) = strip_comment_prefix(line, prefixes);
    if prefix.is_empty() || line.contains(['\r', '\n']) {
        return None;
    }
    // A longer marker run (Rust `////`) is a different comment class:
    // re-applying the profile marker would turn it into documentation.
    if prefix
        .trim_end()
        .as_bytes()
        .last()
        .is_some_and(|marker_end| body.as_bytes().first() == Some(marker_end))
    {
        return None;
    }
    let tail = &source[content_end..];
    let terminator = if tail.starts_with("\r\n") {
        2
    } else if tail.starts_with('\n') {
        1
    } else if tail.is_empty() {
        0
    } else {
        return None;
    };

    Some((
        start..content_end + terminator,
        prefix.trim_end_matches(' '),
    ))
}

#[cfg(test)]
mod tests {
    use super::comment_runs_protected;
    use rstest::rstest;

    #[rstest]
    #[case::rust("// first\n// protected\nstruct Cache {}\n// sibling\n", "rs")]
    #[case::csharp("// first\n// protected\nclass Cache {}\n// sibling\n", "cs")]
    fn comment_runs_should_skip_whole_overlapping_run(#[case] source: &str, #[case] ext: &str) {
        let marker = source.find("protected").unwrap();
        let protected = marker..marker + "protected".len();

        let runs = comment_runs_protected(source, ext, &["//"], &[protected]);

        assert_eq!(runs.len(), 1);
        assert_eq!(&source[runs[0].bytes.clone()], "// sibling\n");
    }
}
