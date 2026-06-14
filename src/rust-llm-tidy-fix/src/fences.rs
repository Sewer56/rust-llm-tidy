//! Rewrite nested markdown fences to alternate marker characters.
//!
//! [`fix_fences`] scans `input` for fenced code blocks. When one fence
//! directly contains another, it rewrites the inner fence's marker to the
//! opposite character (backticks <-> tildes) so a nested fence cannot close
//! the outer block early. The outer (depth-0) marker is always preserved.
//!
//! This mirrors the doc-comment handling of [`crate::tables::fix_tables`]: a
//! leading `///` or `//!` prefix is stripped, the fence is processed, and the
//! prefix is re-applied.
//!
//! The function is idempotent: a document already using outer-backtick /
//! inner-tilde alternation is returned unchanged (as a borrowed [`Cow`]).

use crate::tables::{split_terminator, strip_doc_prefix};
use std::borrow::Cow;

/// A fence currently open on the nesting stack.
struct OpenFence {
    /// Marker character of the opener as it appears in the source.
    source_marker: char,
    /// Run length of the opener's marker run.
    run_len: usize,
    /// Marker this fence (and its matching closer) must use after rewriting.
    expected_marker: char,
}

/// Rewrite nested markdown fences to alternate backtick/tilde markers.
///
/// Only fences nested inside another fence are rewritten; the outer
/// (depth-0) fence keeps its original marker. Run lengths, info strings,
/// and any `///` / `//!` doc-comment prefix are preserved. When no fence
/// changes, the original buffer is borrowed back (idempotent).
pub fn fix_fences(input: &str) -> Cow<'_, str> {
    // Fast path: no fence markers anywhere means nothing can change.
    if !input.contains('`') && !input.contains('~') {
        return Cow::Borrowed(input);
    }

    let mut output = String::with_capacity(input.len());
    let mut changed = false;
    let mut stack: Vec<OpenFence> = Vec::new();

    for segment in input.split_inclusive('\n') {
        let (content, term) = split_terminator(segment);
        let (prefix, body) = strip_doc_prefix(content);

        let stripped = body.trim_start();
        if let Some((source_marker, run_len, info)) = parse_fence(stripped) {
            // Body leading whitespace before the fence run (e.g. an indented
            // fence); preserved verbatim when rebuilding the line.
            let lead = &body[..body.len() - stripped.len()];

            // A closer has an empty info string and matches the top of the
            // stack (same source marker, run length >= the opener's).
            let closer_match = stack
                .last()
                .map(|open| open.source_marker == source_marker && open.run_len <= run_len)
                .unwrap_or(false);
            let is_closer = info.is_empty() && closer_match;

            if is_closer {
                let open = stack.pop().expect("non-empty when closer_match");
                if source_marker != open.expected_marker {
                    emit_fence(
                        &mut output,
                        prefix,
                        lead,
                        open.expected_marker,
                        run_len,
                        info,
                        term,
                    );
                    changed = true;
                } else {
                    output.push_str(segment);
                }
            } else {
                // Opener. Depth-0 keeps its source marker as the root; deeper
                // levels alternate relative to the root (even depth = root,
                // odd depth = opposite).
                let depth = stack.len();
                let root_marker = if depth == 0 {
                    source_marker
                } else {
                    stack[0].source_marker
                };
                let expected_marker = if depth.is_multiple_of(2) {
                    root_marker
                } else {
                    alternate(root_marker)
                };

                if source_marker != expected_marker {
                    emit_fence(
                        &mut output,
                        prefix,
                        lead,
                        expected_marker,
                        run_len,
                        info,
                        term,
                    );
                    changed = true;
                } else {
                    output.push_str(segment);
                }
                stack.push(OpenFence {
                    source_marker,
                    run_len,
                    expected_marker,
                });
            }
        } else {
            // Blank lines and other content are emitted verbatim; the stack
            // is intentionally NOT reset by blank lines (same nesting model
            // as the spec).
            output.push_str(segment);
        }
    }

    if changed {
        Cow::Owned(output)
    } else {
        Cow::Borrowed(input)
    }
}

/// Return the opposite fence marker character.
fn alternate(marker: char) -> char {
    if marker == '`' { '~' } else { '`' }
}

/// Rebuild a fence segment: `prefix` + `lead` + `marker * run_len` + `info`
/// + `term`.
fn emit_fence(
    output: &mut String,
    prefix: &str,
    lead: &str,
    marker: char,
    run_len: usize,
    info: &str,
    term: &str,
) {
    output.push_str(prefix);
    output.push_str(lead);
    output.extend(std::iter::repeat_n(marker, run_len));
    output.push_str(info);
    output.push_str(term);
}

/// Parse a fence from `stripped` if its leading run is 3+ backticks or tildes.
///
/// Returns `(marker, run_len, info)` where `info` is the text after the run
/// (may be empty). Returns `None` for non-fence lines.
fn parse_fence(stripped: &str) -> Option<(char, usize, &str)> {
    let marker = stripped.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let run_len = stripped.chars().take_while(|&c| c == marker).count();
    if run_len < 3 {
        return None;
    }
    // Backticks and tildes are ASCII, so the byte offset equals the run length.
    let info = &stripped[run_len..];
    Some((marker, run_len, info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn no_fence_returns_borrowed() {
        let input = "hello world\nno fences here\n";
        let out = fix_fences(input);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(&*out, input);
    }

    #[test]
    fn rewrites_inner_backtick_to_tilde() {
        // Spec example (TODO-1): outer ```text with an inner ```rust block.
        let input = "\
```text
text
~~~rust
inner
~~~
```
";
        let expected = "\
```text
text
~~~rust
inner
~~~
```
";
        let out = fix_fences(input);
        assert_eq!(&*out, expected, "inner backticks should flip to tildes");
    }

    #[test]
    fn preserves_doc_comment_prefix() {
        // Nested fences inside `///` doc comments keep their prefix.
        let input = "\
/// ```text
/// ~~~rust
/// inner
/// ~~~
/// ```
";
        let expected = "\
/// ```text
/// ~~~rust
/// inner
/// ~~~
/// ```
";
        let out = fix_fences(input);
        assert_eq!(&*out, expected, "doc-comment prefix must be preserved");
    }

    #[test]
    fn outer_tilde_inner_backtick_kept() {
        // Already alternating (tilde root, backtick inner) -> borrowed.
        let input = "\
~~~text
text
```rust
inner
```
~~~
";
        let out = fix_fences(input);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "already-alternating input should be borrowed"
        );
        assert_eq!(&*out, input);
    }

    #[test]
    fn outer_tilde_inner_tilde_flips_inner_to_backtick() {
        let input = "\
~~~text
text
```rust
inner
```
~~~
";
        let expected = "\
~~~text
text
```rust
inner
```
~~~
";
        let out = fix_fences(input);
        assert_eq!(&*out, expected, "inner tildes should flip to backticks");
    }

    #[test]
    fn preserves_run_length() {
        // A 4-backtick inner fence becomes a 4-tilde fence.
        let input = "\
```text
text
~~~~rust
inner
~~~~
```
";
        let expected = "\
```text
text
~~~~rust
inner
~~~~
```
";
        let out = fix_fences(input);
        assert_eq!(&*out, expected, "run length must be preserved");
    }

    #[test]
    fn preserves_info_string() {
        let input = "\
```text
text
~~~rust,no_run
inner
~~~
```
";
        let out = fix_fences(input);
        assert!(
            out.contains("~~~rust,no_run"),
            "info string must be preserved:\n{out}"
        );
    }

    #[test]
    fn idempotent_on_nested() {
        let input = "\
```text
text
~~~rust
inner
~~~
```
";
        let once = fix_fences(input).into_owned();
        let twice = fix_fences(&once).into_owned();
        assert_eq!(twice, once, "fix_fences must be idempotent");
    }

    #[test]
    fn already_canonical_borrowed() {
        // Outer backtick / inner tilde is canonical -> borrowed unchanged.
        let input = "\
```text
text
~~~rust
inner
~~~
```
";
        let out = fix_fences(input);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "canonical outer-backtick/inner-tilde input should be borrowed"
        );
        assert_eq!(&*out, input);
    }

    #[test]
    fn alternation_resets_to_root_at_depth_2() {
        // Depth 2 (even) reuses the root marker; only odd depths alternate.
        let input = "\
```text
~~~rust
```python
deep
```
~~~
```
";
        let expected = "\
```text
~~~rust
```python
deep
```
~~~
```
";
        let out = fix_fences(input);
        assert_eq!(
            &*out, expected,
            "depth-2 fences should reuse the root marker (backtick), not a third alternation"
        );
    }
}
