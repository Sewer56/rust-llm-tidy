//! Pending-heredoc delimiter parsing for the scanner: reading the
//! delimiter word after a `<<` lead.
//!
//! Recognition style is configuration: the lexicon's [`Heredoc`]
//! selects it; only the queued delimiter lives here.

use super::super::lexicon::{Heredoc, ident_byte, ident_start};

/// One queued heredoc: its delimiter and whether the terminator line
/// may carry a lead (`<<~`, `<<-`).
pub(super) struct PendingHeredoc {
    /// The delimiter word the terminator line must equal (after its
    /// permitted lead).
    pub(super) word: String,
    /// Whether the terminator lead is permitted: `\t` in the shell
    /// family, any whitespace in Ruby.
    pub(super) indented: bool,
}

/// Recognizes a heredoc opener at `bytes[i]` (a `<<` lead).
///
/// Queues the delimiter with whether the terminator may be indented.
/// Returns the consumed width, or `None` when the bytes do not open a
/// heredoc.
pub(super) fn heredoc_open(
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
    let word = core::str::from_utf8(&bytes[word_start..j]).expect("ASCII identifier");
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
