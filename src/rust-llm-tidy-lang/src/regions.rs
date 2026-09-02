//! Preprocessor conditional region scan for C-family languages.
//!
//! [`Regions::scan`] walks the raw source once and assigns every line a
//! region id: each preprocessor directive line starts a new region.
//!
//! Reordering that permutes only within one region id therefore never
//! moves an item across a conditional (`#if`/`#elif`/`#else`/`#endif`) or
//! any other directive boundary.
//!
//! Directives inside comments and string or character literals never start
//! regions: the scan tracks `//` and `/* */` comments, regular and
//! interpolated strings, verbatim strings, and raw strings across lines in
//! one pass.
//!
//! Anything ambiguous fails closed to `None` so callers degrade reordering
//! to a no-op.
//!
//! Ambiguous sources: unbalanced conditionals (an `#endif`/`#else`/
//! `#elif` without a matching `#if`, or an unclosed `#if` at end of
//! file) and a file ending inside an unterminated block comment,
//! verbatim string, or raw string.
//!
//! Interpolated raw strings (`$"""`-form) also reject the scan: their
//! interpolation holes can hold nested literals with quote runs the raw
//! scan cannot safely attribute.
//!
//! Known limitation: interpolation holes in classic `$"..."` strings
//! that contain string literals can desync the scan into a phantom
//! multi-line state.

/// Lexical state carried across lines of the scan.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LexState {
    /// Ordinary code.
    Code,
    /// Inside a `/* */` comment.
    BlockComment,
    /// Inside a regular `"..."` string literal.
    String,
    /// Inside a verbatim `@"..."` string literal.
    VerbatimString,
    /// Inside a `'...'` character literal.
    Char,
    /// Inside a raw string literal opened by a run of `len` `"` quotes.
    RawString {
        /// The opening quote-run length; the literal closes on the next
        /// run of at least this many quotes.
        len: usize,
    },
}

/// Per-line region ids produced by [`Regions::scan`].
///
/// Lines sharing an id form one region; every directive line starts a new
/// id, so ids are assigned in increasing order and items of one id are
/// contiguous in the file.
pub struct Regions {
    /// Region id per line.
    ids: Vec<u32>,
}

impl Regions {
    /// Scan `source` for preprocessor conditional regions.
    ///
    /// Returns `None` when the source is ambiguous for region assignment:
    /// unbalanced conditionals, interpolated raw strings, or the file
    /// ending inside an unterminated block comment, verbatim string, or
    /// raw string (see the module docs).
    pub fn scan(source: &str) -> Option<Self> {
        let mut ids: Vec<u32> = Vec::with_capacity(source.lines().count());
        let mut state = LexState::Code;
        let mut region: u32 = 0;
        let mut depth: u32 = 0;

        for line in source.lines() {
            if state == LexState::Code
                && let Some(directive) = directive_word(line)
            {
                region = region.checked_add(1)?;
                match directive {
                    "if" => depth += 1,
                    "elif" | "else" => {
                        if depth == 0 {
                            return None;
                        }
                    }
                    "endif" => {
                        depth = depth.checked_sub(1)?;
                    }
                    // Other directives (`#define`, `#region`, ...) split
                    // regions without changing conditional nesting: moving
                    // code across one can change what it applies to.
                    _ => {}
                }
                // A directive line is consumed whole: its own comments or
                // strings cannot open a multi-line literal.
                ids.push(region);
                continue;
            }
            ids.push(region);
            state = lex_line(state, line)?;
        }

        if state != LexState::Code || depth != 0 {
            return None;
        }
        Some(Self { ids })
    }

    /// The region id of a 1-based `line` (matching
    /// [`SourceItem::start_line`]
    /// numbering).
    ///
    /// [`SourceItem::start_line`]: rust_llm_tidy_model::parse::SourceItem::start_line
    ///
    /// Lines outside the source map to region `0`.
    pub fn id_of_line(&self, line: usize) -> u32 {
        self.ids.get(line.saturating_sub(1)).copied().unwrap_or(0)
    }
}

/// The directive word of `line` when it is a preprocessor directive line:
/// leading whitespace, `#`, optional whitespace, then an identifier.
///
/// Returns an empty slice for a bare `#` (still a directive line) and
/// `None` for non-directive lines.
fn directive_word(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix('#')?;
    let skipped = rest.len() - rest.trim_start().len();
    let word_end = rest[skipped..]
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(rest.len(), |pos| skipped + pos);
    Some(&rest[skipped..word_end])
}

/// Advance the lexical state across one non-directive line.
///
/// Returns `None` when the line holds a construct the scan cannot lex
/// safely (interpolated raw strings); the caller rejects the whole scan.
///
/// Scans bytes: every delimiter the scanner tracks is ASCII, and UTF-8
/// continuation bytes never collide with ASCII, so byte scanning is safe
/// and allocates nothing per line.
fn lex_line(mut state: LexState, line: &str) -> Option<LexState> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match state {
            LexState::Code => {
                let (next, advanced) = code_step(bytes, i)?;
                state = next;
                i += advanced;
            }
            LexState::BlockComment => {
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    state = LexState::Code;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            // Regular strings and char literals cannot span lines, so their
            // closing quote or the line's end leaves code state; escapes
            // keep the scan inside the literal, and only the matching
            // quote closes it.
            LexState::String => {
                if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
                    i += 2;
                } else if bytes[i] == b'"' {
                    state = LexState::Code;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            LexState::Char => {
                if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
                    i += 2;
                } else if bytes[i] == b'\'' {
                    state = LexState::Code;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            // Verbatim strings may span lines; a doubled quote is an
            // escaped quote, a single quote closes the literal.
            LexState::VerbatimString => {
                if bytes[i] == b'"' && bytes.get(i + 1) == Some(&b'"') {
                    i += 2;
                } else if bytes[i] == b'"' {
                    state = LexState::Code;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            // Raw strings span lines and carry no escapes; a run of at
            // least `len` quotes (consumed whole) closes the literal.
            LexState::RawString { len } => {
                if bytes[i] == b'"' && quote_run_len(bytes, i) >= len {
                    i += quote_run_len(bytes, i);
                    state = LexState::Code;
                } else {
                    i += 1;
                }
            }
        }
    }
    // An unterminated regular string or char literal is invalid source;
    // leaving its state would swallow following directive lines, so the
    // line's end closes it (splitting conservatively).
    Some(match state {
        LexState::String | LexState::Char => LexState::Code,
        state => state,
    })
}

/// One step of code-state scanning at `bytes[i]`; returns the next state
/// and how many bytes were consumed, or `None` to reject the scan.
fn code_step(bytes: &[u8], i: usize) -> Option<(LexState, usize)> {
    // Raw string openers first: a run of 3+ quotes, optionally behind a
    // `$`/`$$`/`@` prefix, so their content never opens comments or
    // regular strings.
    //
    // Interpolated forms (`$` in the prefix) are rejected: their holes
    // can hold nested literals with quote runs the raw scan cannot
    // safely attribute.
    if let Some((len, width, interpolated)) = raw_string_open(bytes, i) {
        if interpolated {
            return None;
        }
        return Some((LexState::RawString { len }, width));
    }
    Some(match bytes[i] {
        // Line comment: the rest of the line is comment text.
        b'/' if bytes.get(i + 1) == Some(&b'/') => (LexState::Code, bytes.len() - i),
        b'/' if bytes.get(i + 1) == Some(&b'*') => (LexState::BlockComment, 2),
        b'@' if bytes.get(i + 1) == Some(&b'"') => (LexState::VerbatimString, 2),
        b'$' if bytes.get(i + 1) == Some(&b'@') && bytes.get(i + 2) == Some(&b'"') => {
            (LexState::VerbatimString, 3)
        }
        b'@' if bytes.get(i + 1) == Some(&b'$') && bytes.get(i + 2) == Some(&b'"') => {
            (LexState::VerbatimString, 3)
        }
        b'$' if bytes.get(i + 1) == Some(&b'"') => (LexState::String, 2),
        b'"' => (LexState::String, 1),
        b'\'' => (LexState::Char, 1),
        _ => (LexState::Code, 1),
    })
}

/// A raw-string opener at `bytes[i]`: an optional run of at most three
/// `$`/`@` prefix bytes followed by a run of at least three `"` quotes.
///
/// Returns the opening quote-run length, the consumed width, and whether
/// the prefix marks an interpolated raw string, or `None` when the bytes
/// at `i` do not open a raw string.
fn raw_string_open(bytes: &[u8], i: usize) -> Option<(usize, usize, bool)> {
    let mut j = i;
    while j < bytes.len() && (bytes[j] == b'$' || bytes[j] == b'@') && j - i < 3 {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'"' {
        let run = quote_run_len(bytes, j);
        if run >= 3 {
            let interpolated = bytes[i..j].contains(&b'$');
            return Some((run, j - i + run, interpolated));
        }
    }
    None
}

/// Length of the run of `"` starting at `i`.
fn quote_run_len(bytes: &[u8], i: usize) -> usize {
    bytes[i..].iter().take_while(|&&b| b == b'"').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Region ids per line for a source.
    fn scan_ids(source: &str) -> Vec<u32> {
        Regions::scan(source).expect("balanced source").ids
    }

    /// A file without directives is one region.
    #[test]
    fn scan_keeps_plain_source_one_region() {
        assert_eq!(scan_ids("int a;\nint b;\n"), vec![0, 0]);
    }

    /// Every conditional directive line starts a new region; balanced
    /// nesting returns ids in order.
    #[test]
    fn scan_splits_regions_on_conditional_directives() {
        let source = concat!(
            "int a;\n",    // 0
            "#if DEBUG\n", // 1
            "int b;\n",    // 1
            "#else\n",     // 2
            "int c;\n",    // 2
            "#endif\n",    // 3
            "int d;\n",    // 3
        );
        assert_eq!(scan_ids(source), vec![0, 1, 1, 2, 2, 3, 3]);
    }

    /// `#elif` counts like `#else`: it must sit inside an open `#if` and
    /// starts a new region.
    #[test]
    fn scan_handles_elif_branches() {
        let source = concat!(
            "#if A\n",   // 1
            "int a;\n",  // 1
            "#elif B\n", // 2
            "int b;\n",  // 2
            "#endif\n",  // 3
        );
        assert_eq!(scan_ids(source), vec![1, 1, 2, 2, 3]);
    }

    /// Nested conditionals get distinct regions per branch.
    #[test]
    fn scan_splits_nested_conditionals() {
        let source = concat!(
            "#if A\n",  // 1
            "int a;\n", // 1
            "#if B\n",  // 2
            "int b;\n", // 2
            "#endif\n", // 3
            "int c;\n", // 3
            "#endif\n", // 4
            "int d;\n", // 4
        );
        assert_eq!(scan_ids(source), vec![1, 1, 2, 2, 3, 3, 4, 4]);
    }

    /// Directives inside line and block comments never start regions or
    /// change nesting.
    #[test]
    fn scan_ignores_directives_inside_comments() {
        let source = concat!(
            "// #if DEBUG\n",
            "/* #if INNER\n",
            "still comment\n",
            "#endif\n",
            "*/\n",
            "int a;\n",
        );
        assert_eq!(scan_ids(source), vec![0; 6]);
    }

    /// Directives inside regular, interpolated, and verbatim strings never
    /// start regions; verbatim strings may span lines and escape quotes by
    /// doubling.
    #[test]
    fn scan_ignores_directives_inside_strings() {
        let source = concat!(
            "var a = \"#if DEBUG\";\n",
            "var b = $\"{x} #endif\";\n",
            "var c = @\"quote \"\" #else\n",
            "still verbatim #endif\";\n",
            "var d = '#';\n",
            "int e;\n",
        );
        assert_eq!(scan_ids(source), vec![0; 6]);
    }

    /// Interpolated verbatim strings span lines in both prefix orders
    /// (`$@"` and `@$"`), so directive-position lines inside them stay
    /// inert.
    ///
    /// The `#endif` lines sit at column 0: mis-reading either prefix
    /// would drop the scanner back to code state, read them as real
    /// unbalanced directives, and reject the scan.
    #[test]
    fn scan_ignores_directives_inside_interpolated_verbatim_strings() {
        let source = concat!(
            "var a = $@\"multi\n",
            "#endif\n",
            "\";\n",
            "var b = @$\"also multi\n",
            "#if X\n",
            "\";\n",
            "int c;\n",
        );
        assert_eq!(scan_ids(source), vec![0; 7]);
    }

    /// Raw string content never opens comments or strings and never
    /// matches directives, so real conditionals after the literal still
    /// split regions.
    ///
    /// The directive-looking content line sits at column 0: mis-reading
    /// the literal would consume it as a real directive and change the
    /// ids.
    #[test]
    fn scan_lexes_raw_strings_and_keeps_real_directives() {
        let source = concat!(
            "var pattern = \"\"\"\n", // 0: raw string opens
            "/* not a comment\n",     // 0: raw content
            "#if NOT_A_DIRECTIVE\n",  // 0: raw content
            "\"\"\";\n",              // 0: raw string closes
            "#if DEBUG\n",            // 1: real directive
            "int a;\n",               // 1
            "#endif\n",               // 2
        );
        assert_eq!(scan_ids(source), vec![0, 0, 0, 0, 1, 1, 2]);
    }

    /// Interpolated raw strings reject the whole scan: their holes can
    /// hold nested literals with quote runs the raw scan cannot safely
    /// attribute, so callers degrade reordering to a no-op.
    ///
    /// The directive pair inside the literal is balanced and sits at
    /// column 0, so a regression that lexed the literal would return
    /// `Some` with split regions instead of the expected `None`.
    #[test]
    fn scan_rejects_interpolated_raw_strings() {
        let source = concat!(
            "var json = $\"\"\"\n",
            "{ \"key\": 1 }\n",
            "#if INNER\n",
            "#endif\n",
            "\"\"\";\n",
            "int a;\n",
        );
        assert!(
            Regions::scan(source).is_none(),
            "interpolated raw string must reject the scan"
        );
    }

    /// Escaped quotes and escapes inside strings and char literals keep the
    /// scanner in-literal.
    ///
    /// The escaped quote before `/*` pins the load-bearing path: broken
    /// escape handling would close the string early, open a block comment,
    /// and swallow the following real directives.
    #[test]
    fn scan_handles_escaped_quotes() {
        let source = concat!(
            "var a = \"quoted \\\" /* not a comment\";\n",
            "#if DEBUG\n",
            "int b;\n",
            "#endif\n",
            "var c = '\\'';\n",
            "int d;\n",
        );
        assert_eq!(scan_ids(source), vec![0, 1, 1, 2, 2, 2]);
    }

    /// Non-conditional directives split regions too: moving code across a
    /// `#define` or `#region` can change what it applies to.
    #[test]
    fn scan_splits_regions_on_other_directives() {
        let source = concat!(
            "#define X\n",       // 1
            "int a;\n",          // 1
            "#region Helpers\n", // 2
            "int b;\n",          // 2
        );
        assert_eq!(scan_ids(source), vec![1, 1, 2, 2]);
    }

    /// Unbalanced conditionals reject the scan so callers degrade to a
    /// no-op: a stray `#endif`/`#else`/`#elif`, an unclosed `#if`, and a
    /// stray `#endif` after a balanced block.
    #[test]
    fn scan_rejects_unbalanced_conditionals() {
        let cases = [
            "#endif\n",
            "#else\n",
            "#elif B\n",
            "#if A\nint a;\n",
            "#if A\n#endif\n#endif\n",
        ];
        for source in cases {
            assert!(
                Regions::scan(source).is_none(),
                "unbalanced source must scan to None: {source:?}"
            );
        }
    }

    /// A file ending inside an unterminated multi-line literal rejects the
    /// scan: walking the rest as code could swallow real directives.
    #[test]
    fn scan_rejects_source_ending_inside_a_literal() {
        let cases = [
            concat!("/* never closed\n", "#if A\n", "#endif\n"),
            concat!("var s = @\"never closed\n", "#if A\n", "#endif\n"),
            concat!("var s = \"\"\"\n", "#if A\n", "#endif\n"),
        ];
        for source in cases {
            assert!(
                Regions::scan(source).is_none(),
                "unterminated literal must scan to None: {source:?}"
            );
        }
    }

    /// `id_of_line` maps 1-based line numbers; out-of-range lines map to 0.
    #[test]
    fn id_of_line_maps_one_based_lines() {
        let regions = Regions::scan("int a;\n#if X\nint b;\n#endif\n").unwrap();

        assert_eq!(regions.id_of_line(1), 0);
        assert_eq!(regions.id_of_line(2), 1);
        assert_eq!(regions.id_of_line(3), 1);
        assert_eq!(regions.id_of_line(4), 2);
        assert_eq!(regions.id_of_line(999), 0);
    }
}
