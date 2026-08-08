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

use realign::realign_table;
use std::borrow::Cow;

mod realign;

/// Realign every GFM table in `input`, preserving any per-line doc-comment
/// prefix (`///`, `//!`, with optional leading indent).
///
/// Non-table lines and lines without a pipe are returned verbatim. When no
/// table changes, the original buffer is borrowed back (idempotent).
///
/// # Arguments
///
/// - `input`: the markdown (or Rust source with `///` / `//!` doc-comment
///   tables) to realign.
///
/// # Allocation strategy
///
/// The output buffer is allocated lazily: a single read-only scan runs first,
/// and only when a table actually changes is a `String` allocated and the
/// unchanged text before it copied in. A fully-aligned document therefore
/// returns [`Cow::Borrowed`] with **zero** heap allocation.
///
/// The scan also fast-forwards over pipe-less regions with [`str::find`]
/// (which the standard library lowers to a vectorized byte search), so files
/// where tables are a small fraction - typical Rust source - are not charged
/// per-line for the surrounding code.
pub fn fix_tables(input: &str) -> Cow<'_, str> {
    // Output buffer, allocated lazily on the first real change. `copied_until`
    // is the byte offset in `input` already present in `output`; the slice
    // `input[copied_until..next_change_start]` is copied in just-in-time.
    let mut output = String::new();
    let mut changed = false;
    let mut copied_until = 0usize;

    let mut pos = 0usize;
    while pos < input.len() {
        // Fast-forward to the start of the next line that contains a pipe,
        // skipping whole runs of pipe-less text/code in a single vectorized
        // byte search. If no pipe remains, nothing can change.
        let line_start = match input[pos..].find('|') {
            None => break,
            Some(rel) => {
                let pipe = pos + rel;
                match input[pos..pipe].rfind('\n') {
                    Some(r) => pos + r + 1,
                    None => pos,
                }
            }
        };

        // Gather the maximal run of consecutive pipe lines sharing one
        // doc-comment prefix, starting at `line_start`.
        let (prefix, bodies, terminators, run_end) = gather_run_from(input, line_start);

        if let Some(realigned) = realign_table(&bodies) {
            // First change: reserve roughly the full input size so subsequent
            // pushes never trigger capacity regrowth.
            if !changed {
                output.reserve(input.len());
            }
            // Flush any unchanged text between the last change and here.
            if line_start > copied_until {
                output.push_str(&input[copied_until..line_start]);
            }
            for (line, term) in realigned.iter().zip(terminators.iter()) {
                output.push_str(prefix);
                output.push_str(line);
                output.push_str(term);
            }
            changed = true;
            copied_until = run_end;
        }
        pos = run_end;
    }

    if changed {
        if copied_until < input.len() {
            output.push_str(&input[copied_until..]);
        }
        output.shrink_to_fit();
        Cow::Owned(output)
    } else {
        Cow::Borrowed(input)
    }
}

/// Split `line` into content and terminator (`\n` or `\r\n`).
pub(crate) fn split_terminator(line: &str) -> (&str, &str) {
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
pub(crate) fn strip_doc_prefix(line: &str) -> (&str, &str) {
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

/// Gather the contiguous run of pipe-bearing lines starting at byte offset
/// `line_start` in `input`, all sharing the first line's doc-comment prefix.
///
/// A *run* is the longest prefix of consecutive lines whose body still
/// contains `|` and whose stripped doc-comment prefix equals the first line's.
/// The first line is assumed to already contain a pipe - the caller
/// ([`fix_tables`]) fast-forwards to one - so it always seeds the run.
/// Folding the gather and the per-line split into a single pass avoids
/// reparsing each line's prefix on the way back out.
///
/// # Returns
///
/// A four-tuple `(prefix, bodies, terminators, run_end)`, one slice per
/// concern:
///
/// **`prefix`** - shared doc-comment prefix from the first line: indent +
/// `///` or `//!` + one space. Empty for plain markdown. Identical across the
/// run, so re-applied to every line on the way out.
///
/// **`bodies`** - one [`Vec`] entry per line, the text after stripping
/// `prefix`. The raw pipe row, e.g. `| a | b |`.
///
/// **`terminators`** - one entry per line, parallel to `bodies` (same index =
/// same line). Verbatim terminator: `\n`, `\r\n`, or `""` for a trailing line
/// without a newline, so the caller round-trips endings exactly.
///
/// **`run_end`** - byte offset of the first line *not* in the run (the one
/// that broke an invariant, or `input.len()` at EOF). Caller resumes scan here.
///
/// `bodies.len() == terminators.len()` always.
///
/// # Examples
///
/// Plain markdown - the run stops where a line has no pipe:
///
/// ```text
/// input:      "| a | b |\nno pipe\n"
/// line_start: 0
/// -> prefix      = ""
///    bodies      = ["| a | b |"]
///    terminators = ["\n"]
///    run_end     = 10  // offset of "no pipe"
/// ```
///
/// Rust doc comment - the shared `/// ` prefix is factored out and both
/// rows join the run:
///
/// ```text
/// input:      "/// | a |\n/// | b |\ncode\n"
/// line_start: 0
/// -> prefix      = "/// "
///    bodies      = ["| a |", "| b |"]
///    terminators = ["\n", "\n"]
///    run_end     = 20  // offset of "code"
/// ```
fn gather_run_from(input: &str, line_start: usize) -> (&str, Vec<&str>, Vec<&str>, usize) {
    let mut cur = line_start;
    let mut bodies: Vec<&str> = Vec::new();
    let mut terminators: Vec<&str> = Vec::new();

    // Seed the run from the first line; its prefix becomes the membership
    // contract every later line must match.
    let seg = next_segment(&input[cur..]);
    let (content, term) = split_terminator(seg);
    let (prefix, body) = strip_doc_prefix(content);
    bodies.push(body);
    terminators.push(term);
    cur += seg.len();

    while cur < input.len() {
        let seg = next_segment(&input[cur..]);
        let (content, term) = split_terminator(seg);
        let (p, b) = strip_doc_prefix(content);
        // Extend only while the next line keeps the same prefix and is still
        // a pipe row; otherwise the run ends just before it.
        if b.contains('|') && p == prefix {
            bodies.push(b);
            terminators.push(term);
            cur += seg.len();
        } else {
            break;
        }
    }
    (prefix, bodies, terminators, cur)
}

/// Return the next line of `s` including its terminator (`\n` or `\r\n`),
/// or the remaining slice for a trailing line without a newline.
#[inline]
fn next_segment(s: &str) -> &str {
    match s.find('\n') {
        Some(idx) => &s[..=idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pipe_returns_borrowed() {
        let input = "\
hello world
no tables here
";
        let out = fix_tables(input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*out, input);
    }

    #[test]
    fn realigns_plain_markdown_table() {
        // Single-line `\n` escapes (not a multi-line `\`-continuation string):
        // the pre-commit hook runs `fix_tables` on `.rs` source, and would
        // realign any multi-line pipe input back to canonical form, silently
        // re-breaking this test. One physical line is not seen as a table.
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
        let input = "\
/// | a   | bb |
/// | --- | -- |
/// | ccc | d  |
pub fn f() {}
";
        let out = fix_tables(input);
        assert!(out.contains("/// | a   | bb |"), "prefix preserved:\n{out}");
        assert!(out.contains("pub fn f() {}"), "code line untouched:\n{out}");
    }

    #[test]
    fn preserves_inner_doc_comment_prefix() {
        let input = "\
//! | a   | b |
//! | --- | - |
//! | ccc | d |
";
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
        let input = "let x = a | b;
";
        let out = fix_tables(input);
        assert_eq!(&*out, input, "non-table pipe line should be unchanged");
    }

    #[test]
    fn idempotent_on_misaligned_input() {
        let input = "\
| a   | bb |
| --- | -- |
| ccc | d  |
";
        let once = fix_tables(input).into_owned();
        let twice = fix_tables(&once).into_owned();
        assert_eq!(twice, once, "fix_tables must be idempotent");
    }

    #[test]
    fn idempotent_on_doc_comment_table() {
        let input = "\
/// | a   | bb |
/// | --- | -- |
/// | ccc | d  |
";
        let once = fix_tables(input).into_owned();
        let twice = fix_tables(&once).into_owned();
        assert_eq!(twice, once, "fix_tables on doc comments must be idempotent");
    }

    #[test]
    fn already_aligned_returns_borrowed() {
        let input = "\
| a   | bb |
| --- | -- |
| ccc | d  |
";
        let out = fix_tables(input);
        assert!(
            matches!(out, std::borrow::Cow::Borrowed(_)),
            "already-aligned table should be borrowed"
        );
        assert_eq!(&*out, input);
    }

    #[test]
    fn multiple_tables_and_text_roundtrip() {
        // Two tables separated by prose: the first is misaligned (realigns),
        // the second is already canonical (borrowed). Exercises the lazy
        // output buffer: unchanged text before, between, and after the changed
        // run must be copied through verbatim.
        let input = "\
intro line
| a  | b |
| -- | - |
| cc | d |

| x  | y |
| -- | - |
| zz | w |
trailer
";
        let out = fix_tables(input).into_owned();
        // first table realigned to width [2, 1].
        assert!(out.contains("| a  | b |"), "{out}");
        assert!(out.contains("| -- | - |"), "{out}");
        assert!(out.contains("| cc | d |"), "{out}");
        // second table was already canonical and is carried through unchanged.
        assert!(out.contains("| x  | y |"), "{out}");
        assert!(out.contains("| zz | w |"), "{out}");
        assert!(
            out.contains("intro line") && out.contains("trailer"),
            "{out}"
        );
        // idempotent
        let twice = fix_tables(&out).into_owned();
        assert_eq!(twice, out, "must be idempotent across mixed runs");
    }

    #[test]
    fn crlf_line_endings_preserved() {
        let input = "| a | b |\r\n| --- | --- |\r\n| cc | d |\r\n";
        let out = fix_tables(input);
        assert!(out.contains("\r\n"), "CRLF must be preserved");
        assert!(out.contains("| a  | b |"), "{out}");
        let owned = out.into_owned();
        let twice = fix_tables(&owned).into_owned();
        assert_eq!(twice, owned, "idempotent under CRLF");
    }

    #[test]
    fn no_trailing_newline() {
        let input = "| a | b |\n| --- | --- |\n| cc | d |";
        let out = fix_tables(input);
        assert!(!out.ends_with('\n'), "no terminator introduced");
        assert!(out.contains("| a  | b |"), "{out}");
    }
}
