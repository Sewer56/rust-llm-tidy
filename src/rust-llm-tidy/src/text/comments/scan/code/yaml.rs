//! YAML token boundaries for the comment scanner.

/// Skip a plain scalar or node property without interpreting embedded quotes.
///
/// Mapping separators and flow delimiters remain visible to the scanner.
/// Block scalar indicators remain visible to its fail-closed rejection.
pub(super) fn plain_end(bytes: &[u8], i: usize, flow: bool) -> Option<usize> {
    match bytes[i] {
        b' ' | b'\t' | b'\'' | b'"' | b'[' | b']' | b'{' | b'}' | b',' | b'|' | b'>' => {
            return None;
        }
        b':' | b'-' | b'?' if separator(bytes, i + 1) => return None,
        // Properties precede a scalar; its opening quote must stay visible.
        b'&' | b'!' => {
            let end = bytes[i..]
                .iter()
                .position(|b| b.is_ascii_whitespace())
                .map_or(bytes.len(), |offset| i + offset);
            return Some(end);
        }
        _ => {}
    }

    let mut end = i + 1;
    while end < bytes.len() {
        if (bytes[end] == b'#' && comment_start(bytes, end))
            || (bytes[end] == b':' && separator(bytes, end + 1))
            || (flow && matches!(bytes[end], b'[' | b']' | b'{' | b'}' | b','))
        {
            break;
        }
        end += 1;
    }

    Some(end)
}

/// Whether `#` starts a comment rather than belonging to a plain scalar.
pub(super) fn comment_start(bytes: &[u8], i: usize) -> bool {
    i == 0 || matches!(bytes[i - 1], b' ' | b'\t')
}

/// A mapping/sequence indicator needs separation from the following scalar.
fn separator(bytes: &[u8], i: usize) -> bool {
    bytes.get(i).is_none_or(|b| b.is_ascii_whitespace())
}
