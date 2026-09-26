//! Insert missing blank lines between documented members.
//!
//! [`fix_spacing`] adds a blank line between adjacent Rust or C# type
//! members if either has docs or attributes. It leaves undocumented
//! pairs and same-line members alone.
//!
//! The line goes before the next member's attached comments and
//! attributes. In C#, it also goes before a leading `#if`.
//!
//! The fix only inserts line endings; running it again changes nothing.

use crate::languages::backend_for;
use crate::source::ItemKind;
use core::ops::Range;
use std::borrow::Cow;

pub(crate) mod csharp;
pub(crate) mod rust;

/// One blank line inserted between adjacent members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpacingEdit {
    /// Byte offset at the start of the next member's first attached line.
    pub byte: usize,

    /// 1-based input line before which the blank line goes.
    pub line: u32,

    /// Kind of the enclosing type or impl.
    pub kind: ItemKind,

    /// Previous member's name, or its kind when unnamed.
    pub prev: Box<str>,

    /// Next member's name, or its kind when unnamed.
    pub next: Box<str>,
}

/// Insert missing blank lines between documented members of Rust or C#
/// source.
///
/// Other extensions return the original text without parsing. The
/// extension match ignores ASCII case. A syntax-error tree also leaves
/// the text unchanged.
///
/// # Arguments
///
/// - `source` - Source text to space.
/// - `ext` - File extension without a dot (`rs` or `cs`).
/// - `protected` - Byte ranges to leave alone. A range overlapping
///   either member prevents an edit between them.
///
/// # Returns
///
/// Source text, borrowed when unchanged, and one [`SpacingEdit`] per
/// inserted blank line in byte order.
///
/// # Errors
///
/// Returns [`anyhow::Error`] if the grammar fails to load, the parser rejects
/// it, or parsing produces no tree. Syntax errors leave the source unchanged.
pub fn fix_spacing<'a>(
    source: &'a str,
    ext: &str,
    protected: &[Range<usize>],
) -> anyhow::Result<(Cow<'a, str>, Vec<SpacingEdit>)> {
    let fix = if ext.eq_ignore_ascii_case("rs") {
        rust::fix_rust
    } else if ext.eq_ignore_ascii_case("cs") {
        csharp::fix_csharp
    } else {
        return Ok((Cow::Borrowed(source), Vec::new()));
    };

    let Some(backend) = backend_for(ext) else {
        return Ok((Cow::Borrowed(source), Vec::new()));
    };
    let parsed = backend.parse(source)?;
    Ok(fix(source, &parsed, protected))
}

/// The start of `node`'s line when only whitespace precedes it.
///
/// Returns `None` when something shares the line with `node`: the
/// attachment is then ambiguous and the gap is skipped.
fn attachment_line_start(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    let byte = node.start_byte();
    // tree-sitter columns count bytes, so the slice stays on char
    // boundaries.
    let line_start = byte - node.start_position().column;
    source[line_start..byte]
        .trim()
        .is_empty()
        .then_some(line_start)
}

/// Applies the collected insertions in one pass over `source`.
///
/// Edits are sorted by byte and deduplicated, then the untouched
/// slices are copied with one line terminator inserted per edit. An
/// empty edit list borrows `source` back unchanged.
fn emit<'a>(source: &'a str, mut edits: Vec<SpacingEdit>) -> (Cow<'a, str>, Vec<SpacingEdit>) {
    edits.sort_unstable_by_key(|edit| edit.byte);
    edits.dedup_by_key(|edit| edit.byte);
    if edits.is_empty() {
        return (Cow::Borrowed(source), edits);
    }

    let mut out = String::with_capacity(source.len() + edits.len() * 2);
    let mut copied = 0;
    for edit in &edits {
        out.push_str(&source[copied..edit.byte]);
        out.push_str(terminator_before(source, edit.byte));
        copied = edit.byte;
    }
    out.push_str(&source[copied..]);
    (Cow::Owned(out), edits)
}

/// True when `gap` contains a complete blank line.
///
/// A blank line is a `\n` followed only by spaces, tabs, or `\r` and
/// another `\n`. Trailing indentation before the next member does not
/// count: it is not a complete line.
fn has_blank_line(gap: &str) -> bool {
    let bytes = gap.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\n' {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

/// True when `range` and the closed byte span `start..=end` share at
/// least one byte.
///
/// Ranges that only touch a span boundary do not block.
fn overlaps_span(range: &Range<usize>, start: usize, end: usize) -> bool {
    range.start <= end && start < range.end
}

/// The line terminator that ends the line just before `byte`.
///
/// A CRLF pair is detected by looking back two bytes; everything else
/// takes `\n`. Insertion bytes always follow a newline, but the guard
/// keeps the helper total.
fn terminator_before(source: &str, byte: usize) -> &'static str {
    let bytes = source.as_bytes();
    if byte >= 2 && bytes[byte - 1] == b'\n' && bytes[byte - 2] == b'\r' {
        "\r\n"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Runs the dispatcher over `source` with no protected ranges.
    fn fixed<'a>(source: &'a str, ext: &str) -> (Cow<'a, str>, Vec<SpacingEdit>) {
        fix_spacing(source, ext, &[]).expect("fixture parses")
    }

    // ── dispatch ──

    #[rstest]
    #[case::markdown("# Title\n", "md")]
    #[case::python("class A:\n    pass\n", "py")]
    #[case::empty("", "")]
    fn fix_spacing_should_borrow_when_the_extension_is_unsupported(
        #[case] source: &str,
        #[case] ext: &str,
    ) {
        let (text, edits) = fixed(source, ext);

        assert!(matches!(text, Cow::Borrowed(_)));
        assert_eq!(edits, Vec::new());
    }

    #[rstest]
    #[case::rust(
        "struct S {\n    /// d a.\n    a: u32,\n    /// d b.\n    b: u32,\n}\n",
        "RS"
    )]
    #[case::csharp(
        "class C {\n    /// d a.\n    int a;\n    /// d b.\n    int b;\n}\n",
        "CS"
    )]
    fn fix_spacing_should_dispatch_case_insensitively(#[case] source: &str, #[case] ext: &str) {
        let (text, edits) = fixed(source, ext);

        assert!(matches!(text, Cow::Owned(_)));
        assert_eq!(edits.len(), 1);
    }

    #[test]
    fn fix_spacing_should_borrow_when_the_tree_recovered_from_an_error() {
        let (text, edits) = fixed(
            "struct S {\n    /// d a.\n    a: u32,\n    /// d b.\n    b: u32\n",
            "rs",
        );

        assert!(matches!(text, Cow::Borrowed(_)));
        assert_eq!(edits, Vec::new());
    }
}
