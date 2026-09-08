//! The open-literal state steps of the comment scanner.
//!
//! Each step method consumes one byte inside one open literal: a block
//! comment, quote, backtick template, triple-quoted string, or template
//! hole. `super`'s `scan_line` dispatches to it.

use super::super::lexicon::Syntax;
use super::{Scanner, State, Step, close_run, push_block_line};
use crate::rules::lint::{Dialect, DocRegion};
use core::mem;

impl<'a> Scanner<'a> {
    /// One Block-state byte: the closer emits the block's region, a
    /// nested opener rejects the scan, any other byte advances.
    pub(super) fn block_step(&mut self, raw: &str, i: usize, number: usize) -> Option<Step> {
        let bytes = raw.as_bytes();
        let (open, close) = self.lex.block.expect("block state implies a pair");
        // Alone-marker families close only on a lone closer
        // line (MATLAB); a mid-line `%}` is block content.
        if bytes[i..].starts_with(close.as_bytes())
            && (!self.lex.block_markers_alone
                || (raw[..i].trim().is_empty() && raw[i + close.len()..].trim().is_empty()))
        {
            push_block_line(
                &raw[self.seg_start..i],
                self.block_opener,
                number,
                &mut self.block_lines,
            );
            self.block_opener = false;
            close_run(&mut self.run, &mut self.regions);
            self.regions.push(DocRegion {
                dialect: Dialect::BlockDoc,
                lines: mem::take(&mut self.block_lines),
            });
            self.state = State::Code;
            self.seg_start = i + close.len();
            return Some(Step::Advanced(close.len()));
        }
        // A nested opener is ambiguous (Swift, Haskell,
        // Elm, and Scheme nest; C, Lua, SQL, and MATLAB
        // do not): fail closed.
        if bytes[i..].starts_with(open.as_bytes()) {
            return None;
        }
        Some(Step::Advanced(1))
    }

    /// One Quote-state byte: escapes skip two bytes, the closing quote
    /// returns to code, anything else advances one.
    pub(super) fn quote_step(&mut self, bytes: &[u8], i: usize, double: bool) -> Option<Step> {
        let quote = if double { b'"' } else { b'\'' };
        let escape = match self.lex.syntax {
            Syntax::PowerShell => double.then_some(b'`'),
            Syntax::Yaml => double.then_some(b'\\'),
            Syntax::Common => Some(b'\\'),
        };

        // Interpolated expressions may contain nested quotes.
        if self.lex.syntax == Syntax::PowerShell && double && bytes[i..].starts_with(b"$(") {
            return None;
        }

        if (Some(bytes[i]) == escape && bytes.get(i + 1).is_some())
            || (self.lex.syntax != Syntax::Common && !double && bytes[i..].starts_with(b"''"))
        {
            return Some(Step::Advanced(2));
        }
        if bytes[i] == quote {
            self.state = State::Code;
        }
        Some(Step::Advanced(1))
    }

    /// One Backtick-state byte: escapes skip two bytes, the closing
    /// backtick returns to code, `${` opens a template hole.
    pub(super) fn backtick_step(&mut self, bytes: &[u8], i: usize) -> Option<Step> {
        if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
            Some(Step::Advanced(2))
        } else if bytes[i] == b'`' {
            self.state = State::Code;
            Some(Step::Advanced(1))
        } else if self.lex.template_holes && bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            self.state = State::Hole { depth: 1 };
            Some(Step::Advanced(2))
        } else {
            Some(Step::Advanced(1))
        }
    }

    /// One Triple-state byte: escapes skip two bytes, the closing fence
    /// returns to code, anything else advances one.
    pub(super) fn triple_step(&mut self, raw: &str, i: usize, single: bool) -> Option<Step> {
        let bytes = raw.as_bytes();
        let fence: &[u8] = if single { b"'''" } else { b"\"\"\"" };
        if bytes[i] == b'\\' && bytes.get(i + 1).is_some() {
            Some(Step::Advanced(2))
        } else if bytes[i..].starts_with(fence) {
            self.state = State::Code;
            Some(Step::Advanced(3))
        } else {
            Some(Step::Advanced(1))
        }
    }

    /// One Hole-state byte by brace depth: `{` and `}` track the
    /// nesting, `}` at depth one returns to the backtick literal.
    pub(super) fn hole_step(&mut self, bytes: &[u8], i: usize, depth: u32) -> Option<Step> {
        match bytes[i] {
            // Nested literals inside a hole are outside the
            // modeled lexicon: fail closed.
            b'"' | b'\'' | b'`' => None,
            b'{' => {
                self.state = State::Hole { depth: depth + 1 };
                Some(Step::Advanced(1))
            }
            b'}' => {
                self.state = if depth == 1 {
                    State::Backtick
                } else {
                    State::Hole { depth: depth - 1 }
                };
                Some(Step::Advanced(1))
            }
            _ => Some(Step::Advanced(1)),
        }
    }
}
