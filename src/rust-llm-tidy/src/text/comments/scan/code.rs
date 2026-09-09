//! The Code-state checks of the comment scanner.
//!
//! The `Scanner` is always in one lexical state: ordinary code, or
//! inside something already open - a block comment, string, or template
//! hole. This file handles the ordinary-code state; the `steps` module
//! handles the open ones.
//!
//! In the Code state, `scan_line` in `super` hands each byte to
//! `code_checks`, which runs the family's checks in a fixed order.
//!
//! The first check that applies consumes the byte. A check that does
//! not apply passes it to the next check.
//!
//! Order matters: PowerShell escapes must run before a marker can claim
//! the byte, and block openers win over line markers. When no check
//! applies, `code_step` advances one byte of plain code.
//!
//! Checks can also reject the scan: an unmodeled literal form aborts
//! it, and the caller reports nothing rather than guess.

use super::super::lexicon::{
    Heredoc, Lexicon, Syntax, comment_starts_word, ident_byte, ident_start,
};
use super::super::yaml;
use super::heredoc::{PendingHeredoc, heredoc_open};
use super::{Scanner, State, Step, close_run};
use crate::rules::lint::{Dialect, DocRegion, RegionLine};

impl<'a> Scanner<'a> {
    /// One Code-state byte: the ordered family checks, then the
    /// single-byte fallback.
    ///
    /// A check answers `Advanced(0)` when it does not apply here; the
    /// next check then runs.
    pub(super) fn code_checks(&mut self, raw: &str, i: usize, number: usize) -> Option<Step> {
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
                yaml::comment_start(bytes, i)
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
        match yaml::plain_end(bytes, i, self.yaml_flow_depth > 0) {
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
