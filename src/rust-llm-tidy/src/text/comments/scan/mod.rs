//! The fail-closed scanner: one linear pass over the source tracking
//! comments and the family's string forms into [`DocRegion`]s.
//!
//! Ambiguity rejects the scan (see the [`super`] module docs): zero
//! regions, never guessed measurement.
//!
//! # Layout
//!
//! - `code` - the Code-state checks of the `Scanner`.
//! - `heredoc` - pending-delimiter parsing for queued heredocs.
//! - `steps` - the open-literal state steps of the `Scanner`.
//!
//! [`DocRegion`]: crate::rules::lint::DocRegion

use super::lexicon::{Heredoc, Lexicon};
use crate::rules::lint::{DocRegion, RegionLine};
use core::ops::Range;
use heredoc::PendingHeredoc;

mod code;
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
