//! Realign GitHub-Flavored Markdown (GFM) tables in place.
//!
//! [`fix_tables`] scans `input` for contiguous pipe-delimited tables and
//! re-pads every column so the delimiters and cell borders line up. It is
//! pure text processing with no Markdown parser dependency, and works on two
//! kinds of input:
//!
//! - Plain markdown (`.md`): tables are realigned directly.
//! - Rust doc comments (`.rs`): a leading `///` or `//!` prefix (with optional
//!   indent and one separating space) is stripped, the table is realigned, and
//!   the prefix is re-applied. Surrounding code is left untouched.
//!
//! [`fix_tables_for`] is the generalized form: it takes the language's
//! line-comment markers instead of the fixed `///` / `//!` pair.
//!
//! Pass the markers longest first (e.g. `["///", "//"]`); tables inside `//`,
//! `#`, `--`, `;`, or `%` comments then realign the same way.
//!
//! The function is idempotent: an already-aligned table is returned unchanged
//! (the returned [`Cow`] is borrowed).
//!
//! # Example
//!
//! ```rust
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
//! assert_eq!(fix_tables(input), expected);
//!
//! // Idempotent: an already-aligned table is borrowed back.
//! assert!(matches!(fix_tables(expected), std::borrow::Cow::Borrowed(_)));
//! ```

use realign::realign_table;
use std::borrow::Cow;

mod realign;

/// Rust doc-comment markers, longest first.
///
/// The fixed prefix family behind [`fix_tables`] and [`crate::fences::fix_fences`];
/// the crate's Rust-facing passes share it so their output stays byte-identical
/// to the generalized `_for` forms.
pub(crate) const DOC_PREFIXES: &[&str] = &["///", "//!"];

/// Realign every GFM table in `input`, preserving any per-line doc-comment
/// prefix (`///`, `//!`, with optional leading indent).
///
/// Non-table lines and lines without a pipe are returned verbatim. When no
/// table changes, the input is borrowed back unchanged (idempotent).
///
/// Delegates to [`fix_tables_for`] with the Rust doc-comment markers
/// `["///", "//!"]`: plain markdown tables realign directly, tables inside
/// `///` / `//!` comments realign with their prefix re-applied.
///
/// # Arguments
///
/// - `input`: the markdown (or Rust source with `///` / `//!` doc-comment
///   tables) to realign.
pub fn fix_tables(input: &str) -> Cow<'_, str> {
    fix_tables_for(input, DOC_PREFIXES)
}

/// Strip an optional line-comment prefix from `line`.
///
/// `prefixes` lists the language's comment markers longest first; the first
/// marker that matches after the leading indent wins, so a longer marker
/// (e.g. `///`) beats a shorter one it starts with (e.g. `//`).
///
/// Returns `(prefix, rest)`: `prefix` is the leading indent (spaces and
/// tabs) plus the matched marker and one separating space when present;
/// `rest` is the remainder of the line.
///
/// Lines that match no marker get an empty prefix and the full line back.
///
/// # Arguments
///
/// - `line`: one input line with its terminator already removed.
/// - `prefixes`: the language's line-comment markers, longest first.
///
/// # Examples
///
/// ```rust
/// use rust_llm_tidy_fix::strip_comment_prefix;
///
/// let (prefix, body) = strip_comment_prefix("  # | a |", &["#"]);
/// assert_eq!(prefix, "  # ");
/// assert_eq!(body, "| a |");
///
/// // Longest marker first: `///` wins over `//`.
/// let (prefix, _) = strip_comment_prefix("/// doc", &["///", "//"]);
/// assert_eq!(prefix, "/// ");
/// ```
#[inline(always)]
pub fn strip_comment_prefix<'a>(line: &'a str, prefixes: &[&str]) -> (&'a str, &'a str) {
    let bytes = line.as_bytes();
    let mut marker = 0usize;
    while marker < bytes.len() && matches!(bytes[marker], b' ' | b'\t') {
        marker += 1;
    }
    let rest = &line[marker..];
    // Longest first: the caller orders `prefixes` so a longer marker (e.g.
    // `///`) is tried before a shorter marker it starts with (`//`).
    if let Some(&matched) = prefixes.iter().find(|&&p| rest.starts_with(p)) {
        let mut body = marker + matched.len();
        if line[body..].starts_with(' ') {
            body += 1;
        }
        (&line[..body], &line[body..])
    } else {
        ("", line)
    }
}

/// Realign every GFM table in `input` for one line-comment prefix family.
///
/// Generalized form of [`fix_tables`]: after the leading indent, each line's
/// comment marker comes from `prefixes` instead of the fixed Rust `///` /
/// `//!` pair.
///
/// The matched marker, its indent, and one separating space (when present)
/// are re-applied to every realigned row.
///
/// A run of pipe lines joins one table only while every line keeps the same
/// stripped prefix, so mixed-prefix runs never form a table.
///
/// Non-table lines and lines without a pipe are returned verbatim. When no
/// table changes, the input is borrowed back unchanged (idempotent).
///
/// # Arguments
///
/// - `input`: the source text to realign.
/// - `prefixes`: the language's line-comment markers, longest first (e.g.
///   `["///", "//"]`) so a longer marker wins over a shorter one it starts
///   with. An empty slice strips nothing (plain markdown mode).
///
/// # Allocation strategy
///
/// The output buffer is allocated lazily: a single read-only scan runs first,
/// and only when a table actually changes is a `String` allocated and the
/// unchanged text before it copied in.
///
/// A fully-aligned document therefore returns a [`Cow::Borrowed`] with
/// **zero** heap allocation.
///
/// The scan also fast-forwards over pipe-less regions with [`str::find`]
/// (which the standard library lowers to a vectorized byte search).
///
/// Files where tables are a small fraction - typical commented source - are
/// therefore not charged per-line for the surrounding code.
///
/// # Example
///
/// ```rust
/// use rust_llm_tidy_fix::fix_tables_for;
///
/// // A GFM table inside `#` comments realigns with the marker kept.
/// let input = "# | a | bb |\n# | ---- | ---- |\n# | ccc | d |\n";
/// let expected = "# | a   | bb |\n# | --- | -- |\n# | ccc | d  |\n";
/// assert_eq!(fix_tables_for(input, &["#"]), expected);
/// ```
pub fn fix_tables_for<'a>(input: &'a str, prefixes: &[&str]) -> Cow<'a, str> {
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
        // comment prefix, starting at `line_start`.
        let (prefix, bodies, terminators, run_end) = gather_run_from(input, line_start, prefixes);

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
#[inline(always)]
pub(crate) fn split_terminator(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    if len == 0 || bytes[len - 1] != b'\n' {
        return (line, "");
    }
    if len > 1 && bytes[len - 2] == b'\r' {
        (&line[..len - 2], "\r\n")
    } else {
        (&line[..len - 1], "\n")
    }
}

/// Strip an optional Rust doc-comment prefix from `line`.
///
/// Returns `(prefix, rest)` where `prefix` is the leading indent plus the
/// marker (`///` or `//!`) and one separating space. Lines without a doc
/// marker get an empty prefix.
#[inline(always)]
pub(crate) fn strip_doc_prefix(line: &str) -> (&str, &str) {
    strip_comment_prefix(line, DOC_PREFIXES)
}

/// Gather the contiguous run of pipe-bearing lines starting at byte offset
/// `line_start` in `input`, all sharing the first line's comment prefix.
///
/// A *run* is the longest prefix of consecutive lines whose body still
/// contains `|` and whose stripped comment prefix (from `prefixes`) equals
/// the first line's.
///
/// The first line is assumed to already contain a pipe - the caller
/// ([`fix_tables_for`]) fast-forwards to one - so it always seeds the run.
///
/// Folding the gather and the per-line split into a single pass avoids
/// reparsing each line's prefix on the way back out.
///
/// # Returns
///
/// A four-tuple `(prefix, bodies, terminators, run_end)`, one slice per
/// concern:
///
/// **`prefix`** - shared comment prefix from the first line: indent + one
/// matched marker from `prefixes` + one space. Empty for plain markdown.
/// Identical across the run, so re-applied to every line on the way out.
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
fn gather_run_from<'a>(
    input: &'a str,
    line_start: usize,
    prefixes: &[&str],
) -> (&'a str, Vec<&'a str>, Vec<&'a str>, usize) {
    let mut cur = line_start;
    let mut bodies: Vec<&str> = Vec::new();
    let mut terminators: Vec<&str> = Vec::new();

    // Seed the run from the first line; its prefix becomes the membership
    // contract every later line must match.
    let seg = next_segment(&input[cur..]);
    let (content, term) = split_terminator(seg);
    let (prefix, body) = strip_comment_prefix(content, prefixes);
    bodies.push(body);
    terminators.push(term);
    cur += seg.len();

    while cur < input.len() {
        let seg = next_segment(&input[cur..]);
        let (content, term) = split_terminator(seg);
        let (p, b) = strip_comment_prefix(content, prefixes);
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
        let text = fix_tables(input);
        assert!(matches!(text, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*text, input);
    }

    #[test]
    fn realigns_plain_markdown_table() {
        // Single-line `\n` escapes (not a multi-line `\`-continuation string):
        // the pre-commit hook runs `fix_tables` on `.rs` source, and would
        // realign any multi-line pipe input back to canonical form, silently
        // re-breaking this test.
        //
        // One physical line is not seen as a table.
        let input = "| a | bb |\n| --- | --- |\n| ccc | d |\n";
        let text = fix_tables(input);
        assert!(text != input, "misaligned table should change");
        assert!(
            text.contains("| a   | bb |"),
            "header should be realigned:\n{}",
            text
        );
        assert!(
            text.contains("| --- | -- |"),
            "delimiter should be realigned:\n{}",
            text
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
        let text = fix_tables(input);
        assert!(
            text.contains("/// | a   | bb |"),
            "prefix preserved:\n{}",
            text
        );
        assert!(
            text.contains("pub fn f() {}"),
            "code line untouched:\n{}",
            text
        );
    }

    #[test]
    fn preserves_inner_doc_comment_prefix() {
        let input = "\
//! | a   | b |
//! | --- | - |
//! | ccc | d |
";
        let text = fix_tables(input);
        assert!(
            text.contains("//! | a   | b |"),
            "//! prefix preserved:\n{}",
            text
        );
        assert!(
            text.contains("//! | ccc | d |"),
            "body row preserved:\n{}",
            text
        );
    }

    #[test]
    fn doc_comment_table_realigns() {
        // A misaligned table inside `///` doc comments realigns and keeps its
        // prefix. Written with single-line `\n` escapes so the repo's own
        // `fix_tables` pre-commit hook cannot re-align the literal back to
        // canonical first.
        let input = "/// | name | value |\n/// | ---- | ----- |\n/// | a | 1 |\npub fn f() {}\n";
        let text = fix_tables(input);
        assert!(
            text.contains("/// | a    | 1     |"),
            "doc table should realign its narrow cell:\n{}",
            text
        );
        assert!(text.contains("pub fn f() {}"), "code untouched");
    }

    #[test]
    fn non_table_pipe_lines_unchanged() {
        let input = "let x = a | b;
";
        let text = fix_tables(input);
        assert_eq!(&*text, input, "non-table pipe line should be unchanged");
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
        let text = fix_tables(input);
        assert!(
            matches!(text, std::borrow::Cow::Borrowed(_)),
            "already-aligned table should be borrowed"
        );
        assert_eq!(&*text, input);
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
        let text = fix_tables(input);
        assert!(text.contains("| a  | b |"), "{text}");
        assert!(text.contains("| -- | - |"), "{text}");
        assert!(text.contains("| cc | d |"), "{text}");
        // second table was already canonical and is carried through unchanged.
        assert!(text.contains("| x  | y |"), "{text}");
        assert!(text.contains("| zz | w |"), "{text}");
        assert!(
            text.contains("intro line") && text.contains("trailer"),
            "{text}"
        );
        // idempotent
        let twice = fix_tables(&text).into_owned();
        assert_eq!(twice, text, "must be idempotent across mixed runs");
    }

    #[test]
    fn two_misaligned_tables_both_realign() {
        // Two adjacent misaligned tables are two separate runs, so both are
        // realigned. Blank line between them so they are distinct runs.
        //
        // Written with single-line `\n` escapes (see
        // `realigns_plain_markdown_table`) so the repo's own `fix_tables`
        // pre-commit/lint hook cannot re-align the literals back to canonical.
        let input = "| a | bb |\n| -- | -- |\n| c | d |\n\n| x | yy |\n| -- | -- |\n| z | w |\n";
        let text = fix_tables(input).into_owned();
        assert_ne!(text, input, "both misaligned tables should realign");
        assert_eq!(fix_tables(&text), text, "idempotent after both realign");
    }

    #[test]
    fn crlf_line_endings_preserved() {
        let input = "| a | b |\r\n| --- | --- |\r\n| cc | d |\r\n";
        let text = fix_tables(input);
        assert!(text.contains("\r\n"), "CRLF must be preserved");
        assert!(text.contains("| a  | b |"), "{}", text);
        let owned = text.into_owned();
        let twice = fix_tables(&owned).into_owned();
        assert_eq!(twice, owned, "idempotent under CRLF");
    }

    #[test]
    fn no_trailing_newline() {
        let input = "| a | b |\n| --- | --- |\n| cc | d |";
        let text = fix_tables(input);
        assert!(!text.ends_with('\n'), "no terminator introduced");
        assert!(text.contains("| a  | b |"), "{}", text);
    }

    // Prefix-family coverage for `strip_comment_prefix` / `fix_tables_for`.
    //
    // Inputs are built with `format!` from single-line `\n`-escaped templates
    // so the repo's own `fix_tables` pre-commit hook cannot realign the
    // literals back to canonical form (same trick as the tests above).

    /// One line-comment family per entry: the marker family and a label for
    /// assertion messages.
    const PREFIX_FAMILIES: [(&str, &str); 5] = [
        ("//", "slash"),
        ("#", "hash"),
        ("--", "dash"),
        (";", "semicolon"),
        ("%", "percent"),
    ];

    #[test]
    fn strip_comment_prefix_matches_each_family_marker() {
        // The marker plus one separating space is stripped after the indent.
        for (marker, label) in PREFIX_FAMILIES {
            let line = format!("  {marker} | a |");
            let (prefix, body) = strip_comment_prefix(&line, &[marker]);
            assert_eq!(prefix, format!("  {marker} "), "{label} prefix");
            assert_eq!(body, "| a |", "{label} body");
        }
    }

    #[test]
    fn strip_comment_prefix_without_marker_or_space() {
        // A marker with no following space still strips just the marker.
        let (prefix, body) = strip_comment_prefix("#| a |", &["#"]);
        assert_eq!((prefix, body), ("#", "| a |"));

        // A line matching no marker keeps an empty prefix and the full line.
        let (prefix, body) = strip_comment_prefix("let x = 1;", &["#", "//"]);
        assert_eq!((prefix, body), ("", "let x = 1;"));
    }

    #[test]
    fn strip_comment_prefix_prefers_longest_marker() {
        // With doc and plain markers in one family, `///` wins before `//`.
        let prefixes = ["///", "//"];
        let (prefix, body) = strip_comment_prefix("/// | a |", &prefixes);
        assert_eq!((prefix, body), ("/// ", "| a |"));
        let (prefix, body) = strip_comment_prefix("// | a |", &prefixes);
        assert_eq!((prefix, body), ("// ", "| a |"));
    }

    #[test]
    fn table_run_stops_at_a_pipe_line_with_a_different_marker() {
        // A `//` row directly after a `///` table row ends the run: each
        // marker keeps its own rows and both tables realign independently.
        //
        // Single-line `\n` escapes (see `realigns_plain_markdown_table`) so
        // the repo's own `fix_tables` hook cannot re-align the literals.
        let input = "/// | a | bb |\n/// | ---- | ---- |\n/// | ccc | d |\n// | x | yy |\n// | -- | -- |\n// | z | w |\n";
        let expected = "/// | a   | bb |\n/// | --- | -- |\n/// | ccc | d  |\n// | x | yy |\n// | - | -- |\n// | z | w  |\n";
        let once = fix_tables_for(input, &["///", "//"]);
        assert_eq!(&*once, expected, "each marker keeps its own rows");
        let twice = fix_tables_for(&once, &["///", "//"]);
        assert_eq!(&*twice, &*once, "must stay idempotent across split runs");
    }

    #[test]
    fn empty_prefix_family_strips_nothing() {
        // `&[]` is plain-markdown mode: no marker is stripped, so the `///`
        // rows are not a valid GFM table and must come back verbatim.
        let input = "/// | a | bb |\n/// | ---- | ---- |\n/// | ccc | d |\n";
        let out = fix_tables_for(input, &[]);
        assert_eq!(&*out, input, "an empty family must strip no marker");
    }

    #[test]
    fn prefix_family_tables_realign_with_marker_and_indent_kept() {
        // One misaligned GFM table per line-comment family: every row comes
        // back with the marker and indent re-applied, the code line stays
        // untouched, and a second pass is a no-op.
        for (marker, label) in PREFIX_FAMILIES {
            let input = format!(
                "  {m} | a | bb |\n  {m} | ---- | ---- |\n  {m} | ccc | d |\ncode();\n",
                m = marker
            );
            let expected = format!(
                "  {m} | a   | bb |\n  {m} | --- | -- |\n  {m} | ccc | d  |\ncode();\n",
                m = marker
            );
            let once = fix_tables_for(&input, &[marker]);
            assert_eq!(
                &*once, expected,
                "table inside {label} comments must realign with prefix kept"
            );
            let twice = fix_tables_for(&once, &[marker]);
            assert_eq!(
                &*twice, &*once,
                "fix_tables_for must be idempotent for {label}"
            );
        }
    }

    #[test]
    fn prefix_family_tables_preserve_crlf() {
        // Every row keeps its CRLF terminator through realignment, and the
        // realigned output is idempotent under CRLF.
        for (marker, label) in PREFIX_FAMILIES {
            let input = format!(
                "{m} | a | bb |\r\n{m} | ---- | ---- |\r\n{m} | ccc | d |\r\n",
                m = marker
            );
            let once = fix_tables_for(&input, &[marker]).into_owned();
            assert_eq!(
                once.matches('\n').count(),
                once.matches("\r\n").count(),
                "every newline must stay CRLF for {label}: {once:?}"
            );
            let last_row = format!("{marker} | ccc | d  |\r\n");
            assert!(once.contains(last_row.as_str()), "{label}: {once:?}");
            let twice = fix_tables_for(&once, &[marker]).into_owned();
            assert_eq!(twice, once, "idempotent under CRLF for {label}");
        }
    }

    #[test]
    fn prefix_family_table_keeps_escaped_pipe() {
        // An escaped `\|` stays inside its cell under a `#` family, so the
        // column count still validates against the delimiter row.
        let input = "# | a\\|b | c |\n# | ----- | -- |\n# | d | e |\n";
        let expected = "# | a\\|b | c |\n# | ---- | - |\n# | d    | e |\n";
        let once = fix_tables_for(input, &["#"]);
        assert_eq!(&*once, expected, "escaped pipe must stay one cell");
        assert_eq!(&*fix_tables_for(&once, &["#"]), &*once, "idempotent");
    }
}
