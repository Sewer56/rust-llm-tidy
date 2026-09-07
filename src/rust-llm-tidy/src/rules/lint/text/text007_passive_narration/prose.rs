//! Collect prose words without matching code or link targets.

/// A word and its original byte offset, for checking intervening punctuation.
pub(super) struct Word<'a> {
    pub text: &'a str,
    pub offset: usize,
}

/// Whether only whitespace separates every word in a candidate phrase.
pub(super) fn contiguous(line: &str, words: &[Word<'_>]) -> bool {
    words.windows(2).all(|pair| {
        line[pair[0].offset + pair[0].text.len()..pair[1].offset]
            .chars()
            .all(char::is_whitespace)
    })
}

/// Collect prose words without joining across punctuation or excluded content.
///
/// Backtick runs of equal length delimit code; an unmatched opener carries to
/// the next measured line. Link destinations and reference labels stay opaque.
pub(super) fn words<'a>(line: &'a str, code_delimiter: &mut usize) -> Vec<Word<'a>> {
    let mut words = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if ch == '\\' && *code_delimiter == 0 {
            chars.next();
            continue;
        }
        if ch == '`' {
            let mut run = 1;
            while chars.next_if(|&(_, next)| next == '`').is_some() {
                run += 1;
            }
            if *code_delimiter == 0 {
                *code_delimiter = run;
            } else if *code_delimiter == run {
                *code_delimiter = 0;
            }
            continue;
        }
        if *code_delimiter != 0 {
            continue;
        }

        // Destinations may nest parentheses; escapes do not close them.
        if ch == ']'
            && let Some((_, open @ ('(' | '['))) = chars.peek().copied()
        {
            chars.next();
            let close = if open == '(' { ')' } else { ']' };
            let mut depth = 1;
            while let Some((_, next)) = chars.next() {
                if next == '\\' {
                    chars.next();
                } else if next == open {
                    depth += 1;
                } else if next == close {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            continue;
        }

        // Autolinks and HTML tags are not prose; bare URLs end at whitespace.
        if ch == '<' {
            for (_, next) in chars.by_ref() {
                if next == '>' {
                    break;
                }
            }
            continue;
        }
        if line[offset..].starts_with("https://") || line[offset..].starts_with("http://") {
            while chars.next_if(|&(_, next)| !next.is_whitespace()).is_some() {}
            continue;
        }

        if ch.is_alphanumeric() || ch == '_' {
            let mut end = offset + ch.len_utf8();
            while let Some((next_offset, next)) =
                chars.next_if(|&(_, next)| next.is_alphanumeric() || next == '_')
            {
                end = next_offset + next.len_utf8();
            }
            words.push(Word {
                text: &line[offset..end],
                offset,
            });
        }
    }
    words
}
