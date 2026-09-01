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
use crate::{FixAnchor, FixKind, FixOutcome};
use scan::is_fence_candidate;
use std::borrow::Cow;
// Re-exported for `crate::fences::parse_fence` callers (e.g. `links`).
pub(crate) use scan::parse_fence;

mod scan;

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
/// changes, the outcome's `text` borrows the original buffer back
/// (idempotent).
///
/// Each flipped opener/closer delimiter line contributes one [`FixAnchor`]
/// at that line.
///
/// # Arguments
///
/// - `input`: the markdown (or Rust source with `///` / `//!` doc comments)
///   to scan for nested fenced code blocks.
pub fn fix_fences(input: &str) -> FixOutcome<'_> {
    // Fast path: no fence markers anywhere means nothing can change.
    if !input.contains('`') && !input.contains('~') {
        return FixOutcome {
            text: Cow::Borrowed(input),
            anchors: Vec::new(),
        };
    }

    // Output is allocated lazily: only once the first segment that needs to
    // change is found. Until then the input is borrowed verbatim.
    //
    // The overwhelmingly common case (already-canonical input, or an
    // idempotent re-run) pays zero output-buffer allocation and zero copying.
    //
    // The two costs that remain are the marker presence check above and a
    // cheap per-line scan.
    //
    // Because `fix_fences` only swaps `` ` `` <-> `~` marker characters, the
    // output length always equals `input.len()`, so `String::with_capacity`
    // never reallocates once allocated.
    let mut out: Option<String> = None;
    let mut stack: Vec<OpenFence> = Vec::new();
    let mut anchors: Vec<FixAnchor> = Vec::new();
    // Byte offset of the start of the current segment within `input`; used to
    // back-fill the verbatim prefix when the first change is emitted.
    let mut pos = 0usize;

    // `split_inclusive('\n')` yields one line per segment; the zip counter is
    // the 1-based line of each segment, used to anchor flipped delimiters.
    for (line_num, segment) in (1u32..).zip(input.split_inclusive('\n')) {
        let seg_start = pos;
        pos += segment.len();

        // Cheap candidate check: a line can only be a fence after
        // [`strip_doc_prefix`] + trim if, ignoring leading whitespace, it begins
        // with a marker run or a `///` / `//!` doc prefix. The vast majority of
        // lines (code, prose) fail this and are emitted verbatim with no
        // further work. See [`is_fence_candidate`] for the exactness argument.
        if !is_fence_candidate(segment) {
            if let Some(o) = out.as_mut() {
                o.push_str(segment);
            }
            continue;
        }

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
                    let o = ensure_output(&mut out, input, seg_start);
                    emit_fence(o, prefix, lead, open.expected_marker, run_len, info, term);
                    anchors.push(FixAnchor {
                        line: line_num,
                        kind: FixKind::Fence,
                    });
                } else if let Some(o) = out.as_mut() {
                    o.push_str(segment);
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
                    let o = ensure_output(&mut out, input, seg_start);
                    emit_fence(o, prefix, lead, expected_marker, run_len, info, term);
                    anchors.push(FixAnchor {
                        line: line_num,
                        kind: FixKind::Fence,
                    });
                } else if let Some(o) = out.as_mut() {
                    o.push_str(segment);
                }
                stack.push(OpenFence {
                    source_marker,
                    run_len,
                    expected_marker,
                });
            }
        } else if let Some(o) = out.as_mut() {
            // Candidate line that did not parse as a fence (e.g. `/// //` or a
            // one/two marker run); emit verbatim.
            o.push_str(segment);
        }
    }

    match out {
        // Something changed: return the rewritten buffer.
        Some(o) => FixOutcome {
            text: Cow::Owned(o),
            anchors,
        },
        // No fence changed: borrow the input back unchanged (zero allocation).
        None => FixOutcome {
            text: Cow::Borrowed(input),
            anchors: Vec::new(),
        },
    }
}

/// Return the opposite fence marker character.
#[inline]
fn alternate(marker: char) -> char {
    if marker == '`' { '~' } else { '`' }
}

/// Rebuild a fence segment: `prefix` + `lead` + `marker * run_len` + `info`
/// + `term`.
#[inline]
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

/// Lazily allocate `out` if absent, copying the verbatim prefix
/// `input[..seg_start]`, then return the mutable buffer.
///
/// Until the first changed segment, [`fix_fences`] borrows the input; this
/// defers the one allocation to the point it is actually needed.
#[inline]
fn ensure_output<'a>(out: &'a mut Option<String>, input: &str, seg_start: usize) -> &'a mut String {
    out.get_or_insert_with(|| {
        let mut s = String::with_capacity(input.len());
        s.push_str(&input[..seg_start]);
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn no_fence_returns_borrowed() {
        let input = "hello world\nno fences here\n";
        let out = fix_fences(input);
        assert!(matches!(out.text, Cow::Borrowed(_)));
        assert!(out.anchors.is_empty(), "no fence -> no anchor");
        assert_eq!(&*out.text, input);
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
        assert_eq!(
            &*out.text, expected,
            "inner backticks should flip to tildes"
        );
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
        assert_eq!(&*out.text, expected, "doc-comment prefix must be preserved");
    }

    #[test]
    fn flips_record_one_anchor_per_delimiter_line() {
        // A nested backtick-inside-backtick fence: both the inner opener and
        // the inner closer flip to tildes, each on its own line. Written with
        // single-line `\n` escapes so the repo's own `fix_fences` lint hook
        // cannot canonicalize the dirty literals before the test runs.
        let input = "```text\ntext\n```rust\ninner\n```\n```\n";
        let expected = "```text\ntext\n~~~rust\ninner\n~~~\n```\n";
        let out = fix_fences(input);
        assert_eq!(
            &*out.text, expected,
            "inner backticks should flip to tildes"
        );
        assert_eq!(
            out.anchors,
            [
                FixAnchor {
                    line: 3,
                    kind: FixKind::Fence,
                },
                FixAnchor {
                    line: 5,
                    kind: FixKind::Fence,
                },
            ],
            "opener and closer each anchor their own delimiter line"
        );
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
            matches!(out.text, Cow::Borrowed(_)),
            "already-alternating input should be borrowed"
        );
        assert!(out.anchors.is_empty(), "no flip -> no anchor");
        assert_eq!(&*out.text, input);
    }

    #[test]
    fn outer_tilde_inner_backtick_noop() {
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
        assert_eq!(
            &*out.text, expected,
            "inner backtick under tilde root stays backtick (already canonical)"
        );
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
        assert_eq!(&*out.text, expected, "run length must be preserved");
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
            out.text.contains("~~~rust,no_run"),
            "info string must be preserved:\n{}",
            out.text
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
        let once = fix_fences(input).text.into_owned();
        let twice = fix_fences(&once).text.into_owned();
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
            matches!(out.text, Cow::Borrowed(_)),
            "canonical outer-backtick/inner-tilde input should be borrowed"
        );
        assert_eq!(&*out.text, input);
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
            &*out.text, expected,
            "depth-2 fences should reuse the root marker (backtick), not a third alternation"
        );
    }

    #[test]
    fn unicode_whitespace_before_fence_is_processed() {
        // A fence run preceded by Unicode whitespace (here form feed `\u{c}`)
        // must still be parsed: the candidate-check fast path uses the same
        // `trim_start` whitespace notion as the full pipeline, so it cannot
        // skip such a line (which would desync the nesting stack).
        let input = "\u{c}```text\n\u{c}```rust\n\u{c}```\n```\n";
        let expected = "\u{c}```text\n\u{c}~~~rust\n\u{c}~~~\n```\n";
        let out = fix_fences(input);
        assert_eq!(
            &*out.text, expected,
            "form-feed-prefixed fences must be processed like space-prefixed ones"
        );
        assert!(
            matches!(out.text, Cow::Owned(_)),
            "the inner backtick fence should flip to tildes"
        );
        assert_eq!(
            out.anchors.len(),
            2,
            "flipped opener and closer each produce an anchor"
        );
    }

    /// Reference: the exact `fix_fences` from commit bc51750 (eager allocation,
    /// no candidate-check fast path), retained only to differential-test that
    /// the optimized version produces byte-identical output and the same
    /// `Cow` variant for every input.
    ///
    /// It shares the module's `parse_fence` / `alternate` / `emit_fence` /
    /// `split_terminator` / `strip_doc_prefix`, which are logic-identical to
    /// bc51750 (the byte-based `parse_fence` equals the old char-based one
    /// because fence markers are ASCII).
    #[allow(clippy::too_many_lines)]
    fn fix_fences_ref(input: &str) -> Cow<'_, str> {
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
                let lead = &body[..body.len() - stripped.len()];
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
                output.push_str(segment);
            }
        }
        if changed {
            Cow::Owned(output)
        } else {
            Cow::Borrowed(input)
        }
    }

    #[test]
    fn optimized_matches_bc51750_reference() {
        // Broad differential corpus: ASCII + Unicode leading whitespace before
        // fences (the fast-path risk area), doc-comment fences, tilde roots,
        // run lengths, info strings, closers with leading ws, unbalanced and
        // non-fence edge cases.
        let cases: &[&str] = &[
            "",
            "no markers here at all\n",
            "single line, no trailing newline",
            // canonical / dirty plain fences
            "```text\ntext\n~~~rust\ninner\n~~~\n```\n",
            "```text\ntext\n```rust\ninner\n```\n```\n",
            // ASCII indentation before fences
            "  ```text\n  ```rust\n  ```\n  ```\n",
            "\t```text\n\t```rust\n\t```\n\t```\n",
            // ASCII oddities (vertical tab, form feed, CR) before fences
            "\u{b}```text\n\u{b}```rust\n\u{b}```\n```\n",
            "\u{c}```text\n\u{c}```rust\n\u{c}```\n```\n",
            "\r```text\n\r```rust\n\r```\n\r```\n",
            // Non-ASCII (Unicode) whitespace before fences
            "\u{a0}```text\n\u{a0}```rust\n\u{a0}```\n```\n",
            "\u{3000}```text\n\u{3000}```rust\n\u{3000}```\n```\n",
            // Closer alone preceded by Unicode whitespace (reviewer case)
            "```\n```rust\n\u{a0}```\n```\n",
            "```\n```rust\n\u{3000}```\n```\n",
            // doc-comment fences (outer + inner same marker)
            "/// ```text\n/// ```rust\n/// inner\n/// ```\n/// ```\n",
            "//! ```text\n//! ```rust\n//! ```\n//! ```\n",
            // tilde root, backtick inner
            "~~~text\n```rust\n```\n~~~\n",
            // run-length preservation (4-backtick outer, 3 inner)
            "````text\n```rust\n```\n````\n",
            // info strings
            "```text\n```rust,no_run\nx\n```\n```\n",
            // unbalanced / edge
            "```text\n",
            "```\n```\n",
            // lines beginning with `/` that are not doc comments
            "// not a doc comment\n```text\n```\n",
            "//// also not a doc comment\n```text\n```\n",
            "/path/like/this\n```text\n```\n",
            // non-ASCII leading non-whitespace (defers to full pipeline)
            "é```text\n```\n",
            "café and code\n```text\n```\n",
        ];
        for &input in cases {
            let got = fix_fences(input);
            let want = fix_fences_ref(input);
            assert_eq!(
                &*got.text, &*want,
                "byte-identical divergence from bc51750 for input {input:?}"
            );
            assert_eq!(
                matches!(got.text, Cow::Borrowed(_)),
                matches!(want, Cow::Borrowed(_)),
                "Cow variant divergence from bc51750 for input {input:?}"
            );
            // The optimized version must remain idempotent on each input.
            let once = fix_fences(input).text.into_owned();
            let twice = fix_fences(&once).text.into_owned();
            assert_eq!(twice, once, "not idempotent for input {input:?}");
        }
    }

    #[test]
    fn optimized_matches_reference_on_generated_inputs() {
        // Deterministic linear congruential generator (LCG; no external test
        // dependency) builds many inputs from a fence-flavoured fragment
        // alphabet: ASCII and Unicode leading whitespace, doc prefixes,
        // mixed markers, run lengths, and info strings.
        //
        // The optimized `fix_fences` must stay byte-identical to the bc51750
        // reference for every generated input.
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            seed
        };
        let frags: &[&str] = &[
            "```text\n",
            "```\n",
            "```rust\n",
            "```rust,no_run\n",
            "````\n",
            "`````\n",
            "~~~\n",
            "~~~text\n",
            "~~~rust\n",
            "/// ```text\n",
            "/// ```\n",
            "/// ~~~\n",
            "//! ```text\n",
            "  ```text\n",
            "\t```\n",
            "\u{c}```\n",
            "\u{b}```\n",
            "\u{a0}```\n",
            "\u{3000}```\n",
            "inner content\n",
            "    indented code\n",
            "\n",
            "plain text\n",
            "//// comment\n",
            "// plain comment\n",
            "é unicode line\n",
            "```python\nx = 1\n```\n",
        ];
        for _ in 0..8192 {
            let n = 1 + (next() as usize) % 16;
            let mut input = String::new();
            for _ in 0..n {
                input.push_str(frags[(next() as usize) % frags.len()]);
            }
            let got = fix_fences(&input);
            let want = fix_fences_ref(&input);
            assert_eq!(
                &*got.text, &*want,
                "byte-identical divergence from bc51750 for generated input {input:?}"
            );
            assert_eq!(
                matches!(got.text, Cow::Borrowed(_)),
                matches!(want, Cow::Borrowed(_)),
                "Cow variant divergence for generated input {input:?}"
            );
        }
    }

    #[test]
    fn crlf_doc_comment_fences_preserved() {
        // CRLF input with nested fences inside a `///` doc comment: only the
        // inner backtick fence should flip to tildes; every line ending must
        // stay `\r\n` (the pass only swaps marker chars, never line terminators
        // - `split_inclusive('\n')` keeps `\r\n` in-segment and `emit_fence`
        // reuses the segment's terminator).
        let input = "/// ```text\r\n/// ```rust\r\n/// inner\r\n/// ```\r\n/// ```\r\n";
        let expected = "/// ```text\r\n/// ~~~rust\r\n/// inner\r\n/// ~~~\r\n/// ```\r\n";
        let out = fix_fences(input);
        assert_eq!(
            &*out.text, expected,
            "inner flips to tildes, CRLF preserved"
        );
        assert_eq!(
            out.text.matches('\n').count(),
            out.text.matches("\r\n").count(),
            "every newline must be CRLF: {out:?}"
        );
        assert_eq!(
            out.anchors,
            [
                FixAnchor {
                    line: 2,
                    kind: FixKind::Fence,
                },
                FixAnchor {
                    line: 4,
                    kind: FixKind::Fence,
                },
            ],
            "flipped opener/closer anchor their doc-comment lines"
        );
    }
}
