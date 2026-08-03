//! Dominant line-ending detection for source-preserving transforms.
//!
//! `dominant_line_ending` returns the line terminator (`"\r\n"` or `"\n"`)
//! that occurs most often in a source string, so text-transform passes can
//! keep the source's line endings on in-place writes instead of hardcoding
//! `"\n"`.

/// Dominant line ending of `source`. CRLF when CRLF breaks are at least as
/// common as bare LF (and at least one CRLF exists); else LF. Rust source
/// only uses `\n` or `\r\n`, so lone `\r` is ignored. A source with no
/// newlines defaults to LF.
///
/// # Arguments
///
/// - `source`: the text whose line endings are examined.
pub fn dominant_line_ending(source: &str) -> &'static str {
    let crlf = source.matches("\r\n").count();
    let lf = source.matches('\n').count().saturating_sub(crlf);
    if crlf > 0 && crlf >= lf { "\r\n" } else { "\n" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lf() {
        assert_eq!(dominant_line_ending("a\nb\nc\n"), "\n");
    }

    #[test]
    fn crlf() {
        assert_eq!(dominant_line_ending("a\r\nb\r\nc\r\n"), "\r\n");
    }

    #[test]
    fn crlf_majority_wins() {
        // 2 CRLF + 1 bare LF -> CRLF (CRLF is at least as common as LF).
        assert_eq!(dominant_line_ending("a\r\nb\r\nc\n"), "\r\n");
    }

    #[test]
    fn crlf_equal_to_lf_still_crlf() {
        // 1 CRLF + 1 bare LF -> CRLF (the >= boundary: at least as common).
        assert_eq!(dominant_line_ending("a\r\nb\n"), "\r\n");
    }

    #[test]
    fn lf_majority_wins() {
        // 2 bare LF + 1 CRLF -> LF (LF strictly more common than CRLF).
        assert_eq!(dominant_line_ending("a\nb\nc\r\n"), "\n");
    }

    #[test]
    fn no_newline_defaults_to_lf() {
        assert_eq!(dominant_line_ending("no newlines at all"), "\n");
    }

    #[test]
    fn lone_cr_ignored() {
        // A lone \r (not followed by \n) is ignored; LF stays dominant.
        assert_eq!(dominant_line_ending("a\rb\nc\n"), "\n");
    }
}
