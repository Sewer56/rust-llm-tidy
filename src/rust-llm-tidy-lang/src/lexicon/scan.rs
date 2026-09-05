//! The fail-closed scanner: one linear pass over the source tracking
//! comments and the family's string forms into [`DocRegion`]s.
//!
//! Ambiguity rejects the scan (see the [`super`] module docs): zero
//! regions, never guessed measurement.
//!
//! [`DocRegion`]: rust_llm_tidy_lint::check::DocRegion

use super::families::{Heredoc, Lexicon, comment_starts_word, ident_byte, ident_start};
use rust_llm_tidy_lint::check::{Dialect, DocRegion, RegionLine};

/// One queued heredoc: its delimiter and whether the terminator line
/// may carry a lead (`<<~`, `<<-`).
struct PendingHeredoc {
    /// The delimiter word the terminator line must equal (after its
    /// permitted lead).
    word: String,
    /// Whether the terminator lead is permitted: `\t` in the shell
    /// family, any whitespace in Ruby.
    indented: bool,
}

/// Lexical state carried across the lines of one scan.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Ordinary code.
    Code,
    /// Inside a block comment.
    Block,
    /// Inside a `"..."` or '...' string; `double` selects the closing
    /// quote, and `carried` marks one that legally spans lines.
    Quote { double: bool, carried: bool },
    /// Inside a backtick literal (multi-line).
    Backtick,
    /// Inside a `"""` or `'''` string (multi-line); `single` selects the
    /// closing fence.
    Triple { single: bool },
    /// Inside a `${...}` interpolation hole of a backtick template, by
    /// brace depth.
    Hole { depth: u32 },
}

/// Scans `source` into doc regions for `lex`.
///
/// Returns `None` for ambiguous sources (module docs): callers emit no
/// findings rather than guess.
pub(super) fn scan(source: &str, lex: &Lexicon) -> Option<Vec<DocRegion>> {
    let mut regions: Vec<DocRegion> = Vec::new();
    // The open standalone-comment run; every other region closes it so
    // regions stay in source order.
    let mut run: Option<DocRegion> = None;
    // The open block comment's content lines.
    let mut block_lines: Vec<RegionLine> = Vec::new();
    let mut block_opener = false;
    let mut state = State::Code;
    let mut heredocs: Vec<PendingHeredoc> = Vec::new();

    for (idx, raw) in source.lines().enumerate() {
        let number = idx + 1;

        // Heredoc payload: string content until the closing delimiter
        // line: exact for strict heredocs; after the lead its family
        // permits (`\t` shells, any whitespace Ruby) for `<<~`/`<<-`.
        // A missing terminator fails the scan at the end.
        if let Some(pending) = heredocs.first() {
            let terminator = if pending.indented {
                match lex.heredoc {
                    Heredoc::Shell => raw.trim_start_matches('\t'),
                    Heredoc::Ruby => raw.trim_start(),
                    Heredoc::None => raw,
                }
            } else {
                raw
            };
            if terminator == pending.word.as_str() {
                heredocs.remove(0);
            }
            continue;
        }

        let bytes = raw.as_bytes();
        // Start of the pending block-comment content on this line.
        let mut seg_start = 0;
        let mut i = 0;
        'chars: while i < bytes.len() {
            match state {
                State::Code => {
                    // Literal forms the family does not model reject the
                    // scan before any marker claims the rest of the line
                    // (SQL `$tag$`, Haskell `[q|`, TeX `\verb`).
                    if lex.rejects.iter().any(|reject| reject.opens(bytes, i)) {
                        return None;
                    }
                    // TeX: an odd-length backslash run before the marker
                    // escapes it, so the marker prints literally; an
                    // even-length run is `\\` commands and the marker
                    // still comments.
                    if lex.escaped_marker && bytes[i] == b'\\' {
                        let mut run = 1;
                        while bytes.get(i + run) == Some(&b'\\') {
                            run += 1;
                        }
                        if bytes.get(i + run) == Some(&lex.line.as_bytes()[0]) {
                            i += if run % 2 == 1 { run + 1 } else { run };
                            continue 'chars;
                        }
                        // Mid-line and no marker: only the run's last
                        // backslash can open a `\verb`-style reject, so
                        // skip straight to it. Line-leading runs keep
                        // the byte-wise walk so the `\\` line-string
                        // rule still sees them.
                        if run > 1 && !raw[..i].trim().is_empty() {
                            i += run - 1;
                            continue 'chars;
                        }
                    }
                    // Block comment opener, ahead of the line marker:
                    // `--[[` and `%{` extend their family's marker.
                    //
                    // Alone-marker families (MATLAB) open only when the
                    // marker is the line's whole content; elsewhere the
                    // line marker comments, as the language reads it.
                    if let Some((open, _)) = lex.block
                        && bytes[i..].starts_with(open.as_bytes())
                        && (!lex.block_markers_alone
                            || (raw[..i].trim().is_empty()
                                && raw[i + open.len()..].trim().is_empty()))
                    {
                        state = State::Block;
                        block_opener = true;
                        seg_start = i + open.len();
                        i = seg_start;
                        continue 'chars;
                    }
                    // Line comment: consumes the rest of the line.
                    // Word-start families (POSIX `#` rules, Ruby after a
                    // token) never open a comment mid-word, so regex
                    // literals and words like `a#b` stay code.
                    if bytes[i..].starts_with(lex.line.as_bytes())
                        && (!lex.word_start_comments || comment_starts_word(bytes, i))
                    {
                        let text = &raw[i + lex.line.len()..];
                        let text = text.trim_start_matches(lex.line.as_bytes()[0] as char);
                        let text = text.strip_prefix(' ').unwrap_or(text);
                        let line = RegionLine {
                            number,
                            text: text.to_string(),
                            indented: text.starts_with('\t') || text.starts_with("    "),
                        };
                        if raw[..i].trim().is_empty() {
                            // Standalone comment lines join one region per
                            // contiguous run.
                            let continues = run.as_ref().is_some_and(|region| {
                                region
                                    .lines
                                    .last()
                                    .is_some_and(|last| last.number + 1 == number)
                            });
                            if continues {
                                run.as_mut().expect("open run").lines.push(line);
                            } else {
                                close_run(&mut run, &mut regions);
                                run = Some(DocRegion {
                                    dialect: Dialect::Markdown,
                                    lines: vec![line],
                                });
                            }
                        } else {
                            // A trailing comment never joins a run: it is
                            // its own region, so fragments on consecutive
                            // lines cannot pool into one paragraph.
                            close_run(&mut run, &mut regions);
                            regions.push(DocRegion {
                                dialect: Dialect::Markdown,
                                lines: vec![line],
                            });
                        }
                        break 'chars;
                    }
                    // Multi-line triple-quoted strings before the
                    // single-quote forms.
                    if lex.triple {
                        if bytes[i..].starts_with(b"\"\"\"") {
                            state = State::Triple { single: false };
                            i += 3;
                            continue 'chars;
                        }
                        if bytes[i..].starts_with(b"'''") {
                            state = State::Triple { single: true };
                            i += 3;
                            continue 'chars;
                        }
                    }
                    match bytes[i] {
                        b'"' => {
                            state = State::Quote {
                                double: true,
                                carried: false,
                            };
                            i += 1;
                        }
                        b'\'' if lex.single_quotes => {
                            state = State::Quote {
                                double: false,
                                carried: false,
                            };
                            i += 1;
                        }
                        b'`' if lex.backtick => {
                            state = State::Backtick;
                            i += 1;
                        }
                        // Zig multi-line strings: a line-leading `\\` run
                        // makes the rest of the line string content. Only
                        // a line lead qualifies, so a `\\` mid-line in
                        // other families never hides a trailing comment.
                        b'\\' if bytes.get(i + 1) == Some(&b'\\') && raw[..i].trim().is_empty() => {
                            break 'chars;
                        }
                        // `$#` and `${#x}` are parameter syntax in `#`
                        // languages, not comment starts.
                        b'$' => {
                            i += match (bytes.get(i + 1), bytes.get(i + 2)) {
                                (Some(&b'#'), _) => 2,
                                (Some(&b'{'), Some(&b'#')) => 3,
                                _ => 1,
                            };
                        }
                        _ => i += code_step(bytes, i, lex, &mut heredocs)?,
                    }
                }
                State::Block => {
                    let (open, close) = lex.block.expect("block state implies a pair");
                    // Alone-marker families close only on a lone closer
                    // line (MATLAB); a mid-line `%}` is block content.
                    if bytes[i..].starts_with(close.as_bytes())
                        && (!lex.block_markers_alone
                            || (raw[..i].trim().is_empty()
                                && raw[i + close.len()..].trim().is_empty()))
                    {
                        push_block_line(&raw[seg_start..i], block_opener, number, &mut block_lines);
                        block_opener = false;
                        close_run(&mut run, &mut regions);
                        regions.push(DocRegion {
                            dialect: Dialect::BlockDoc,
                            lines: std::mem::take(&mut block_lines),
                        });
                        state = State::Code;
                        seg_start = i + close.len();
                        i = seg_start;
                    } else if bytes[i..].starts_with(open.as_bytes()) {
                        // A nested opener is ambiguous (Swift, Haskell,
                        // Elm, and Scheme nest; C, Lua, SQL, and MATLAB
                        // do not): fail closed.
                        return None;
                    } else {
                        i += 1;
                    }
                }
                State::Quote { double, .. } => {
                    let quote = if double { b'"' } else { b'\'' };
                    if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
                        i += 2;
                    } else {
                        if bytes[i] == quote {
                            state = State::Code;
                        }
                        i += 1;
                    }
                }
                State::Backtick => {
                    if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
                        i += 2;
                    } else if bytes[i] == b'`' {
                        state = State::Code;
                        i += 1;
                    } else if lex.template_holes
                        && bytes[i] == b'$'
                        && bytes.get(i + 1) == Some(&b'{')
                    {
                        state = State::Hole { depth: 1 };
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                State::Triple { single } => {
                    let fence: &[u8] = if single { b"'''" } else { b"\"\"\"" };
                    if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
                        i += 2;
                    } else if bytes[i..].starts_with(fence) {
                        state = State::Code;
                        i += 3;
                    } else {
                        i += 1;
                    }
                }
                State::Hole { depth } => match bytes[i] {
                    // Nested literals inside a hole are outside the
                    // modeled lexicon: fail closed.
                    b'"' | b'\'' | b'`' => return None,
                    b'{' => {
                        state = State::Hole { depth: depth + 1 };
                        i += 1;
                    }
                    b'}' => {
                        state = if depth == 1 {
                            State::Backtick
                        } else {
                            State::Hole { depth: depth - 1 }
                        };
                        i += 1;
                    }
                    _ => i += 1,
                },
            }
        }

        // End of line: close line-local states, carry multi-line ones.
        match state {
            State::Block => {
                push_block_line(&raw[seg_start..], block_opener, number, &mut block_lines);
                block_opener = false;
            }
            State::Quote { double, carried } => {
                // A quote continues onto the next line when the family's
                // strings span lines or the line ends in a backslash
                // continuation; a carried quote persists until it closes.
                // Anything else closes here (invalid source; the desync
                // stays line-local).
                if !carried {
                    state = if lex.multiline_quotes || raw.ends_with('\\') {
                        State::Quote {
                            double,
                            carried: true,
                        }
                    } else {
                        State::Code
                    };
                }
            }
            // A hole reaching the line's end is indistinguishable from an
            // unterminated one: fail closed.
            State::Hole { .. } => return None,
            _ => {}
        }
    }

    if state != State::Code || !heredocs.is_empty() {
        return None;
    }
    close_run(&mut run, &mut regions);
    Some(regions)
}

/// Flushes the open standalone-comment run into `regions`, if any.
fn close_run(run: &mut Option<DocRegion>, regions: &mut Vec<DocRegion>) {
    if let Some(region) = run.take() {
        regions.push(region);
    }
}

/// One code-state step at `bytes[i]`: heredoc openers, the fail-closed
/// pattern rejects, or a single-byte advance.
///
/// Returns the consumed width, or `None` to reject the scan.
fn code_step(
    bytes: &[u8],
    i: usize,
    lex: &Lexicon,
    heredocs: &mut Vec<PendingHeredoc>,
) -> Option<usize> {
    match bytes[i] {
        b'<' => {
            // PHP heredocs/nowdocs.
            if lex.line == "//" && bytes[i..].starts_with(b"<<<") {
                return None;
            }
            if lex.heredoc != Heredoc::None && bytes[i..].starts_with(b"<<") {
                // A bare Ruby `<<word` is the push/shift operator in
                // operand position and a heredoc opener after `=`:
                // ambiguous, so fail closed.
                let marked = matches!(
                    bytes.get(i + 2),
                    Some(b'~') | Some(b'-') | Some(b'"') | Some(b'\'')
                );
                let ident = matches!(bytes.get(i + 2), Some(&b) if ident_start(b));
                if lex.heredoc == Heredoc::Ruby && !marked && ident {
                    return None;
                }
                if let Some(width) = heredoc_open(bytes, i, lex.heredoc, heredocs) {
                    return Some(width);
                }
            }
            Some(1)
        }
        // Swift raw multi-line strings.
        b'#' if lex.line == "//" && bytes[i..].starts_with(b"#\"\"\"") => None,
        // C++ raw strings (`R"( ... )"`, custom delimiters included).
        b'R' if lex.line == "//"
            && bytes.get(i + 1) == Some(&b'"')
            && (i == 0 || !ident_byte(bytes[i - 1])) =>
        {
            None
        }
        _ => Some(1),
    }
}

/// Adds one block-comment content segment to the open block's lines.
///
/// The opener line's own marker stars (`/***`) vanish here; continuation
/// lines keep their `*` for the block doc dialect to strip.
fn push_block_line(seg: &str, opener: bool, number: usize, lines: &mut Vec<RegionLine>) {
    let text = if opener {
        seg.trim_start_matches('*').trim()
    } else {
        seg.trim()
    };
    lines.push(RegionLine {
        number,
        text: text.to_string(),
        // The dialect derives indented examples after `*`-stripping.
        indented: false,
    });
}

/// Recognizes a heredoc opener at `bytes[i]` (a `<<` lead) and queues
/// its delimiter with whether the terminator may be indented. Returns
/// the consumed width, or `None` when the bytes do not open a heredoc.
fn heredoc_open(
    bytes: &[u8],
    i: usize,
    style: Heredoc,
    heredocs: &mut Vec<PendingHeredoc>,
) -> Option<usize> {
    let mut j = i + 2;
    // Shells allow the delimiter as the next word (`cat << EOF`).
    if style == Heredoc::Shell {
        while matches!(bytes.get(j), Some(b' ') | Some(b'\t')) {
            j += 1;
        }
    }
    // `<<-` and `<<~` permit an indented terminator line.
    let mut indented = false;
    if matches!(bytes.get(j), Some(b'-') | Some(b'~')) {
        indented = true;
        j += 1;
        if style == Heredoc::Shell {
            while matches!(bytes.get(j), Some(b' ') | Some(b'\t')) {
                j += 1;
            }
        }
    }
    let quote = match bytes.get(j) {
        Some(&q @ (b'"' | b'\'')) => {
            j += 1;
            Some(q)
        }
        _ => None,
    };
    if !matches!(bytes.get(j), Some(&b) if ident_start(b)) {
        return None;
    }
    let word_start = j;
    while matches!(bytes.get(j), Some(&b) if ident_byte(b)) {
        j += 1;
    }
    let word = std::str::from_utf8(&bytes[word_start..j]).expect("ASCII identifier");
    if let Some(q) = quote {
        if bytes.get(j) != Some(&q) {
            return None;
        }
        j += 1;
    }
    heredocs.push(PendingHeredoc {
        word: word.to_string(),
        indented,
    });
    Some(j - i)
}
