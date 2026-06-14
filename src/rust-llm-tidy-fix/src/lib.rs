//! Realign GitHub-Flavored Markdown (GFM) tables in place.
//!
//! [`fix_tables`] scans `input` for contiguous pipe-delimited tables and
//! re-pads every column so the delimiters and cell borders line up. It is
//! pure text processing with no dependencies, and works on two kinds of input:
//!
//! - Plain markdown (`.md`): tables are realigned directly.
//! - Rust doc comments (`.rs`): a leading `///` or `//!` prefix (with optional
//!   indent and one separating space) is stripped, the table is realigned, and
//!   the prefix is re-applied. Surrounding code is left untouched.
//!
//! The function is idempotent: an already-aligned table is returned unchanged
//! (as a borrowed [`Cow`]).
//!
//! # Example
//!
//! ```rust
//! # use std::borrow::Cow;
//! use rust_llm_tidy_fix::fix_tables;
//!
//! // Each column is padded to its widest cell so the pipes line up.
//! let input = "| a | bb |
//! | --- | --- |
//! | ccc | d |
//! ";
//!
//! let expected = "| a   | bb |
//! | --- | -- |
//! | ccc | d  |
//! ";
//!
//! assert_eq!(fix_tables(input).into_owned(), expected);
//!
//! // Idempotent: an already-aligned table is borrowed back unchanged.
//! assert!(matches!(fix_tables(expected), Cow::Borrowed(_)));
//! ```

use std::borrow::Cow;
use table::realign_table;

mod table;

/// Realign every GFM table in `input`, preserving any per-line doc-comment
/// prefix (`///`, `//!`, with optional leading indent).
///
/// Non-table lines and lines without a pipe are returned verbatim. When no
/// table changes, the original buffer is borrowed back (idempotent).
pub fn fix_tables(input: &str) -> Cow<'_, str> {
    if !input.contains('|') {
        return Cow::Borrowed(input);
    }

    let segments: Vec<&str> = input.split_inclusive('\n').collect();
    let mut output = String::with_capacity(input.len());
    let mut changed = false;
    let mut i = 0;

    while i < segments.len() {
        let (content, _term) = split_terminator(segments[i]);
        let (prefix, body) = strip_doc_prefix(content);

        if body.contains('|') {
            let run_start = i;
            let mut k = i;
            let mut run_bodies: Vec<&str> = Vec::new();
            while k < segments.len() {
                let (c, _) = split_terminator(segments[k]);
                let (p, b) = strip_doc_prefix(c);
                if b.contains('|') && p == prefix {
                    run_bodies.push(b);
                    k += 1;
                } else {
                    break;
                }
            }

            match realign_table(&run_bodies) {
                Some(realigned) => {
                    for (idx, line) in realigned.iter().enumerate() {
                        output.push_str(prefix);
                        output.push_str(line);
                        let (_, term) = split_terminator(segments[run_start + idx]);
                        output.push_str(term);
                    }
                    changed = true;
                }
                None => {
                    for idx in 0..run_bodies.len() {
                        output.push_str(segments[run_start + idx]);
                    }
                }
            }
            i = k;
        } else {
            output.push_str(segments[i]);
            i += 1;
        }
    }

    if changed {
        Cow::Owned(output)
    } else {
        Cow::Borrowed(input)
    }
}

/// Split `line` into content and terminator (`\n` or `\r\n`).
fn split_terminator(line: &str) -> (&str, &str) {
    if let Some(rest) = line.strip_suffix('\n') {
        if let Some(content) = rest.strip_suffix('\r') {
            (content, "\r\n")
        } else {
            (rest, "\n")
        }
    } else {
        (line, "")
    }
}

/// Strip an optional Rust doc-comment prefix from `line`.
///
/// Returns `(prefix, rest)` where `prefix` is the leading indent plus the
/// marker (`///` or `//!`) and one separating space. Lines without a doc
/// marker get an empty prefix.
fn strip_doc_prefix(line: &str) -> (&str, &str) {
    let indent_end = line.len() - line.trim_start_matches([' ', '\t']).len();
    let core = &line[indent_end..];
    if let Some(rest) = core
        .strip_prefix("///")
        .or_else(|| core.strip_prefix("//!"))
    {
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        let prefix_len = line.len() - rest.len();
        (&line[..prefix_len], rest)
    } else {
        ("", line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pipe_returns_borrowed() {
        let input = "hello world\nno tables here\n";
        let out = fix_tables(input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*out, input);
    }

    #[test]
    fn realigns_plain_markdown_table() {
        let input = "| a | bb |\n| --- | --- |\n| ccc | d |\n";
        let out = fix_tables(input);
        assert!(out != input, "misaligned table should change");
        assert!(
            out.contains("| a   | bb |"),
            "header should be realigned:\n{out}"
        );
        assert!(
            out.contains("| --- | -- |"),
            "delimiter should be realigned:\n{out}"
        );
    }

    #[test]
    fn preserves_doc_comment_prefix() {
        let input = "/// | a | bb |\n/// | --- | --- |\n/// | ccc | d |\npub fn f() {}\n";
        let out = fix_tables(input);
        assert!(out.contains("/// | a   | bb |"), "prefix preserved:\n{out}");
        assert!(out.contains("pub fn f() {}"), "code line untouched:\n{out}");
    }

    #[test]
    fn preserves_inner_doc_comment_prefix() {
        let input = "//! | a | b |\n//! | --- | --- |\n//! | ccc | d |\n";
        let out = fix_tables(input);
        assert!(
            out.contains("//! | a   | b |"),
            "//! prefix preserved:\n{out}"
        );
        assert!(
            out.contains("//! | ccc | d |"),
            "body row preserved:\n{out}"
        );
    }

    #[test]
    fn non_table_pipe_lines_unchanged() {
        let input = "let x = a | b;\n";
        let out = fix_tables(input);
        assert_eq!(&*out, input, "non-table pipe line should be unchanged");
    }

    #[test]
    fn idempotent_on_misaligned_input() {
        let input = "| a | bb |\n| --- | --- |\n| ccc | d |\n";
        let once = fix_tables(input).into_owned();
        let twice = fix_tables(&once).into_owned();
        assert_eq!(twice, once, "fix_tables must be idempotent");
    }

    #[test]
    fn idempotent_on_doc_comment_table() {
        let input = "/// | a | bb |\n/// | --- | --- |\n/// | ccc | d |\n";
        let once = fix_tables(input).into_owned();
        let twice = fix_tables(&once).into_owned();
        assert_eq!(twice, once, "fix_tables on doc comments must be idempotent");
    }

    #[test]
    fn already_aligned_returns_borrowed() {
        let input = "| a   | bb |\n| --- | -- |\n| ccc | d  |\n";
        let out = fix_tables(input);
        assert!(
            matches!(out, std::borrow::Cow::Borrowed(_)),
            "already-aligned table should be borrowed"
        );
        assert_eq!(&*out, input);
    }
}
