//! The fail-closed scanner: one linear pass over the source tracking
//! comments and the family's string forms into [`DocRegion`]s.
//!
//! Ambiguity rejects the scan (see the [`super`] module docs): zero
//! regions, never guessed measurement.
//!
//! # Layout
//!
//! - `heredoc` - pending-delimiter parsing for queued heredocs.
//! - `steps` - the open-literal state steps of the `Scanner`.
//!
//! [`DocRegion`]: crate::rules::lint::DocRegion

use super::lexicon::{Heredoc, Lexicon, Syntax, comment_starts_word, ident_byte, ident_start};
use crate::rules::lint::{Dialect, DocRegion, RegionLine};
use core::ops::Range;
use heredoc::{PendingHeredoc, heredoc_open};

mod heredoc;
mod steps;

/// The scan's carried state: the lexicon plus everything one pass
/// accumulates or opens across lines.
struct Scanner<'a> {
    /// Exact comment bytes are collected only for strict text-regex scans.
    comment_spans: Option<Vec<Range<usize>>>,
    /// Current line's absolute byte offset and the open block's start.
    line_offset: usize,
    block_start: usize,
    /// The family's lexical table.
    lex: &'a Lexicon,
    /// Completed regions, in source order.
    regions: Vec<DocRegion>,
    /// The open standalone-comment run; every other region closes it
    /// so regions stay in source order.
    run: Option<DocRegion>,
    /// The open block comment's content lines.
    block_lines: Vec<RegionLine>,
    /// Whether the open block's current line is its opener's.
    block_opener: bool,
    /// The lexical state at the next byte.
    state: State,
    /// Queued heredocs awaiting their terminator lines.
    heredocs: Vec<PendingHeredoc>,
    /// YAML flow nesting depth.
    yaml_flow_depth: usize,
    /// Where the open block's pending content segment starts; reset at
    /// each line's start.
    seg_start: usize,
}

/// One state step's outcome at a byte.
///
/// A Code-state check answers `Advanced(0)` when it does not apply, so
/// the dispatch runs the next check; no step returns it to the byte
/// loop.
#[derive(Clone, Copy)]
enum Step {
    /// Consumed `width` bytes; the byte loop continues.
    Advanced(usize),
    /// The rest of the line is consumed; the end-of-line carry runs.
    LineDone,
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

impl<'a> Scanner<'a> {
    /// Scans one line: queued heredoc payload, the byte loop over the
    /// state dispatch, and the end-of-line carry.
    ///
    /// Returns `None` to reject the scan.
    fn scan_line(&mut self, number: usize, raw: &str) -> Option<()> {
        // Heredoc payload: string content until the closing
        // delimiter line. Strict heredocs take it exact.
        //
        // For `<<~`/`<<-` the terminator may carry the lead its
        // family permits (`\t` shells, any whitespace Ruby).
        //
        // A missing terminator fails the scan at the end.
        if let Some(pending) = self.heredocs.first() {
            let terminator = if pending.indented {
                match self.lex.heredoc {
                    Heredoc::Shell => raw.trim_start_matches('\t'),
                    Heredoc::Ruby => raw.trim_start(),
                    Heredoc::None => raw,
                }
            } else {
                raw
            };
            if terminator == pending.word.as_str() {
                self.heredocs.remove(0);
            }
            return Some(());
        }

        let bytes = raw.as_bytes();
        // Start of the pending block-comment content on this line.
        self.seg_start = 0;
        let mut i = 0;
        'chars: while i < bytes.len() {
            let state = self.state;
            let step = match state {
                State::Code => self.code_checks(raw, i, number)?,
                State::Block => self.block_step(raw, i, number)?,
                State::Quote { double, .. } => self.quote_step(bytes, i, double)?,
                State::Backtick => self.backtick_step(bytes, i)?,
                State::Triple { single } => self.triple_step(raw, i, single)?,
                State::Hole { depth } => self.hole_step(bytes, i, depth)?,
            };
            match step {
                Step::Advanced(width) => i += width,
                Step::LineDone => break 'chars,
            }
        }

        // End of line: close line-local states, carry multi-line ones.
        let state = self.state;
        match state {
            State::Block => {
                push_block_line(
                    &raw[self.seg_start..],
                    self.block_opener,
                    number,
                    &mut self.block_lines,
                );
                self.block_opener = false;
            }
            State::Quote { double, carried } => {
                // A quote continues onto the next line when the family's
                // strings span lines or the line ends in a backslash
                // continuation. A carried quote persists until it closes;
                // anything else closes here (invalid source; the desync
                // stays line-local).
                if !carried {
                    if self.comment_spans.is_some()
                        && !self.lex.multiline_quotes
                        && !raw.ends_with('\\')
                    {
                        return None;
                    }
                    self.state = if self.lex.multiline_quotes || raw.ends_with('\\') {
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
        Some(())
    }

    /// One Code-state byte: the ordered family checks, then the
    /// single-byte fallback.
    ///
    /// A check answers `Advanced(0)` when it does not apply here; the
    /// next check then runs.
    fn code_checks(&mut self, raw: &str, i: usize, number: usize) -> Option<Step> {
        let bytes = raw.as_bytes();
        let mut step = self.powershell_step(bytes, i)?;
        if let Step::Advanced(0) = step {
            step = self.reject_step(bytes, raw, i)?;
        }
        if let Step::Advanced(0) = step {
            step = self.block_open_step(bytes, raw, i)?;
        }
        if let Step::Advanced(0) = step {
            step = self.line_comment_step(raw, i, number)?;
        }
        if let Step::Advanced(0) = step {
            step = self.yaml_step(bytes, i)?;
        }
        if let Step::Advanced(0) = step {
            step = self.literal_open_step(bytes, raw, i)?;
        }
        if let Step::Advanced(0) = step {
            // Slash literals are not modeled. Strict span consumers must not
            // mistake their contents for comments, even at the cost of skips.
            if self.comment_spans.is_some() && bytes[i] == b'/' {
                return None;
            }
            return code_step(bytes, i, self.lex, &mut self.heredocs).map(Step::Advanced);
        }
        Some(step)
    }

    /// The PowerShell preamble: escapes and here-string rejects, ahead
    /// of every marker check.
    fn powershell_step(&self, bytes: &[u8], i: usize) -> Option<Step> {
        if self.lex.syntax != Syntax::PowerShell {
            return Some(Step::Advanced(0));
        }
        // PowerShell escapes apply before comment and quote openers.
        if bytes[i] == b'`' {
            return Some(Step::Advanced(1 + usize::from(bytes.get(i + 1).is_some())));
        }
        // Here-strings need their own terminators; reject rather than guess.
        if bytes[i] == b'@' && matches!(bytes.get(i + 1), Some(b'"' | b'\'')) {
            return None;
        }
        Some(Step::Advanced(0))
    }

    /// The fail-closed literal rejects, then the escaped-marker
    /// backslash-run logic.
    fn reject_step(&self, bytes: &[u8], raw: &str, i: usize) -> Option<Step> {
        // Literal forms the family does not model reject
        // the scan early.
        //
        // Rejection happens before any marker claims the
        // rest of the line. Rejected forms: SQL `$tag$`,
        // Haskell `[q|`, TeX `\verb`.
        if self.lex.rejects.iter().any(|reject| reject.opens(bytes, i)) {
            return None;
        }
        // TeX: an odd-length backslash run before the
        // marker escapes it, so the marker prints
        // literally.
        //
        // An even-length run is `\\` commands, so the
        // marker still comments.
        if self.lex.escaped_marker && bytes[i] == b'\\' {
            let mut run = 1;
            while bytes.get(i + run) == Some(&b'\\') {
                run += 1;
            }
            if bytes.get(i + run) == Some(&self.lex.line.as_bytes()[0]) {
                return Some(Step::Advanced(if run % 2 == 1 { run + 1 } else { run }));
            }
            // Mid-line and no marker: skip straight to
            // the run's last backslash.
            //
            // Only it can open a `\verb`-style reject.
            // Line-leading runs keep the byte-wise walk
            // so the `\\` line-string rule still sees
            // them.
            if run > 1 && !raw[..i].trim().is_empty() {
                return Some(Step::Advanced(run - 1));
            }
        }
        Some(Step::Advanced(0))
    }

    /// The block-comment opener, ahead of the line marker: `--[[` and
    /// `%{` extend their family's marker.
    fn block_open_step(&mut self, bytes: &[u8], raw: &str, i: usize) -> Option<Step> {
        let lex = self.lex;
        if let Some((open, _)) = lex.block
            && bytes[i..].starts_with(open.as_bytes())
            && (!lex.block_markers_alone
                || (raw[..i].trim().is_empty() && raw[i + open.len()..].trim().is_empty()))
        {
            self.state = State::Block;
            self.block_start = self.line_offset + i;
            self.block_opener = true;
            self.seg_start = i + open.len();
            return Some(Step::Advanced(open.len()));
        }
        Some(Step::Advanced(0))
    }

    /// The line-comment handler: a standalone line joins or opens the
    /// run, a trailing comment stands as its own region.
    fn line_comment_step(&mut self, raw: &str, i: usize, number: usize) -> Option<Step> {
        let bytes = raw.as_bytes();
        // Line comment: consumes the rest of the line.
        //
        // Word-start families (POSIX `#` rules, Ruby after a token)
        // never open a comment mid-word, so regex literals and words
        // like `a#b` stay code.
        if bytes[i..].starts_with(self.lex.line.as_bytes())
            && (if self.lex.syntax == Syntax::Yaml {
                super::yaml::comment_start(bytes, i)
            } else {
                !self.lex.word_start_comments || comment_starts_word(bytes, i)
            })
        {
            if let Some(spans) = &mut self.comment_spans {
                spans.push(self.line_offset + i..self.line_offset + raw.len());
            }
            let text = &raw[i + self.lex.line.len()..];
            let text = text.trim_start_matches(self.lex.line.as_bytes()[0] as char);
            let text = text.strip_prefix(' ').unwrap_or(text);
            let line = RegionLine {
                number,
                text: text.to_string(),
                indented: text.starts_with('\t') || text.starts_with("    "),
            };
            if raw[..i].trim().is_empty() {
                // Standalone comment lines join one region per
                // contiguous run.
                let continues = self.run.as_ref().is_some_and(|region| {
                    region
                        .lines
                        .last()
                        .is_some_and(|last| last.number + 1 == number)
                });
                if continues {
                    self.run.as_mut().expect("open run").lines.push(line);
                } else {
                    close_run(&mut self.run, &mut self.regions);
                    self.run = Some(DocRegion {
                        dialect: Dialect::Markdown,
                        lines: vec![line],
                    });
                }
            } else {
                // A trailing comment never joins a run: it is its own
                // region, so fragments on consecutive lines cannot
                // pool into one paragraph.
                close_run(&mut self.run, &mut self.regions);
                self.regions.push(DocRegion {
                    dialect: Dialect::Markdown,
                    lines: vec![line],
                });
            }
            return Some(Step::LineDone);
        }
        Some(Step::Advanced(0))
    }

    /// The YAML step: flow-depth tracking, then the plain-scalar skip.
    fn yaml_step(&mut self, bytes: &[u8], i: usize) -> Option<Step> {
        if self.lex.syntax != Syntax::Yaml {
            return Some(Step::Advanced(0));
        }
        match bytes[i] {
            b'[' | b'{' => self.yaml_flow_depth += 1,
            b']' | b'}' => self.yaml_flow_depth = self.yaml_flow_depth.saturating_sub(1),
            _ => {}
        }
        match super::yaml::plain_end(bytes, i, self.yaml_flow_depth > 0) {
            Some(end) => Some(Step::Advanced(end - i)),
            None => Some(Step::Advanced(0)),
        }
    }

    /// The multi-line literal openers: triple quotes, the quote and
    /// backtick forms, Zig line strings, and `$#` parameter syntax.
    fn literal_open_step(&mut self, bytes: &[u8], raw: &str, i: usize) -> Option<Step> {
        // Multi-line triple-quoted strings before the
        // single-quote forms.
        if self.lex.triple {
            if bytes[i..].starts_with(b"\"\"\"") {
                self.state = State::Triple { single: false };
                return Some(Step::Advanced(3));
            }
            if bytes[i..].starts_with(b"'''") {
                self.state = State::Triple { single: true };
                return Some(Step::Advanced(3));
            }
        }
        Some(match bytes[i] {
            b'"' => {
                self.state = State::Quote {
                    double: true,
                    carried: false,
                };
                Step::Advanced(1)
            }
            b'\'' if self.lex.single_quotes => {
                self.state = State::Quote {
                    double: false,
                    carried: false,
                };
                Step::Advanced(1)
            }
            b'`' if self.lex.backtick => {
                self.state = State::Backtick;
                Step::Advanced(1)
            }
            // Zig multi-line strings: a line-leading `\\`
            // run makes the rest of the line string
            // content.
            //
            // Only a line lead qualifies, so a `\\`
            // mid-line in other families never hides a
            // trailing comment.
            b'\\' if bytes.get(i + 1) == Some(&b'\\') && raw[..i].trim().is_empty() => {
                Step::LineDone
            }
            // `$#` and `${#x}` are parameter syntax in `#`
            // languages, not comment starts.
            b'$' => Step::Advanced(match (bytes.get(i + 1), bytes.get(i + 2)) {
                (Some(&b'#'), _) => 2,
                (Some(&b'{'), Some(&b'#')) => 3,
                _ => 1,
            }),
            _ => Step::Advanced(0),
        })
    }
}

/// Return exact comment spans, rejecting unsupported or unfinished literals.
pub(super) fn comment_spans(source: &str, lex: &Lexicon) -> Option<Vec<Range<usize>>> {
    scan_source(source, lex, true)?.comment_spans
}

/// Scans `source` into doc regions for `lex`.
///
/// Returns `None` for ambiguous sources (module docs): callers emit no
/// findings rather than guess.
pub(super) fn scan(source: &str, lex: &Lexicon) -> Option<Vec<DocRegion>> {
    scan_source(source, lex, false).map(|scanner| scanner.regions)
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

/// Share lexical decisions between prose measurement and byte-span consumers.
fn scan_source<'a>(source: &str, lex: &'a Lexicon, strict: bool) -> Option<Scanner<'a>> {
    let mut scanner = Scanner {
        comment_spans: strict.then(Vec::new),
        line_offset: 0,
        block_start: 0,
        lex,
        regions: Vec::new(),
        run: None,
        block_lines: Vec::new(),
        block_opener: false,
        state: State::Code,
        heredocs: Vec::new(),
        yaml_flow_depth: 0,
        seg_start: 0,
    };
    for (idx, line) in source.split_inclusive('\n').enumerate() {
        let raw = line.strip_suffix('\n').unwrap_or(line);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        scanner.scan_line(idx + 1, raw)?;
        scanner.line_offset += line.len();
    }
    if scanner.state != State::Code || !scanner.heredocs.is_empty() {
        return None;
    }
    close_run(&mut scanner.run, &mut scanner.regions);
    Some(scanner)
}

/// Flushes the open standalone-comment run into `regions`, if any.
fn close_run(run: &mut Option<DocRegion>, regions: &mut Vec<DocRegion>) {
    if let Some(region) = run.take() {
        regions.push(region);
    }
}
