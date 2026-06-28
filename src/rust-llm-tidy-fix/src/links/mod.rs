//! Hoist repeated inline links to reference definitions.
//!
//! [`fix_links`] scans `input` for inline links `[text](url)` where the same
//! `(text, url)` pair appears 2+ times, rewrites each occurrence to the
//! reference form `[text]`, and appends exactly one `[text]: url` definition
//! per hoisted link at the end of the document, co-located with any
//! pre-existing trailing reference-definition block.
//!
//! Links used once, autolink (`<...>`) or whitespace/newline-URL forms,
//! already-reference-style links (`[text][ref]`, `[text][]`, `[text]`), links
//! whose text already has a `[text]:` definition, and links inside fenced
//! code blocks are left untouched. Per-line `///` / `//!` doc-comment prefixes
//! are preserved on the rewritten occurrences.
//!
//! The function is idempotent: reference-style output is returned unchanged
//! (as a borrowed [`Cow`]).
//!
//! # Performance
//!
//! The overwhelmingly common input is already canonical - no inline link is
//! repeated - so [`fix_links`] must return [`Cow::Borrowed`] after a single
//! tally pass. That pass jumps between `[` bytes with [`str::find`] (std's
//! optimized search) instead of walking every character, and skips link work
//! entirely for lines without a `[`. The rewrite pass allocates its output
//! lazily (per-segment and overall), so an idempotent re-run or a document
//! with no repeats pays zero allocation and zero copying beyond the tally scan.
//!
//! # Example
//!
//! ```rust
//! # use std::borrow::Cow;
//! use rust_llm_tidy_fix::fix_links;
//!
//! let input = "see [A](http://x) and [A](http://x)\n";
//! let expected = "see [A] and [A]\n[A]: http://x\n";
//! assert_eq!(fix_links(input).into_owned(), expected);
//! assert!(matches!(fix_links(expected), Cow::Borrowed(_)));
//! ```

use crate::tables::{split_terminator, strip_doc_prefix};
use rewrite::{append_definitions, rewrite_links, tally_links};
use scan::{definition_text, step_fence};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

mod rewrite;
mod scan;

/// Hoist repeated inline links `[text](url)` to reference definitions.
///
/// See the module docs for the full rule and constraints. When no link is
/// hoisted, the original buffer is borrowed back (idempotent).
pub fn fix_links(input: &str) -> Cow<'_, str> {
    // Fast path: no link-opening bracket means nothing can change.
    if !input.contains('[') {
        return Cow::Borrowed(input);
    }

    // Pass 1: tally eligible inline links (outside code fences) and record the
    // texts of every existing `[text]:` definition so we never re-define one.
    // Splitting each line once (into content/body) feeds both the fence step
    // and the link tally, and the `contains('[')` guard skips link work for the
    // common bracket-less line.
    let mut fence_stack: Vec<(char, usize)> = Vec::new();
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    let mut order: Vec<(&str, &str)> = Vec::new();
    let mut existing: HashSet<&str> = HashSet::new();
    for segment in input.split_inclusive('\n') {
        let (content, _) = split_terminator(segment);
        let (_, body) = strip_doc_prefix(content);
        if step_fence(&mut fence_stack, body) {
            continue;
        }
        if !fence_stack.is_empty() {
            continue; // inside a code block: no links here
        }
        if !body.contains('[') {
            continue;
        }
        if let Some(key) = definition_text(body) {
            existing.insert(key);
        }
        tally_links(body, &mut counts, &mut order);
    }

    // Hoist set: pairs seen 2+ times whose text is not already defined.
    // `existing.insert(text)` returns false for pre-existing definitions and
    // also dedups by text, so we never emit two `[text]:` lines for one text.
    let mut hoist: Vec<(&str, &str)> = Vec::new();
    let mut hoist_set: HashSet<(&str, &str)> = HashSet::new();
    for &(text, url) in &order {
        if counts[&(text, url)] >= 2 && existing.insert(text) {
            hoist_set.insert((text, url));
            hoist.push((text, url));
        }
    }

    if hoist.is_empty() {
        return Cow::Borrowed(input);
    }

    // Pass 2: rewrite eligible inline links to `[text]`, allocating output
    // lazily on the first changed segment (mirrors `fix_fences`). Lines
    // without a `[` are emitted verbatim without entering the rewriter.
    let mut out: Option<String> = None;
    let mut pos = 0usize;
    fence_stack.clear();
    for segment in input.split_inclusive('\n') {
        let seg_start = pos;
        pos += segment.len();
        let (content, term) = split_terminator(segment);
        let (prefix, body) = strip_doc_prefix(content);
        if step_fence(&mut fence_stack, body) {
            if let Some(o) = out.as_mut() {
                o.push_str(segment);
            }
            continue;
        }
        if !fence_stack.is_empty() {
            if let Some(o) = out.as_mut() {
                o.push_str(segment);
            }
            continue;
        }
        if !body.contains('[') {
            if let Some(o) = out.as_mut() {
                o.push_str(segment);
            }
            continue;
        }
        match rewrite_links(prefix, body, term, &hoist_set) {
            Some(rewritten) => {
                let o = ensure_output(&mut out, input, seg_start);
                o.push_str(&rewritten);
            }
            None => {
                if let Some(o) = out.as_mut() {
                    o.push_str(segment);
                }
            }
        }
    }

    // Append hoisted `[text]: url` definitions at the end of the document.
    let mut buf = out.unwrap_or_else(|| {
        let mut s = String::with_capacity(input.len());
        s.push_str(input);
        s
    });
    append_definitions(&mut buf, &hoist);

    Cow::Owned(buf)
}

/// Lazily allocate `out`, copying the verbatim prefix `input[..seg_start]`.
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
    fn no_bracket_returns_borrowed() {
        let input = "hello world\nno links here\n";
        let out = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(&*out, input);
    }

    #[test]
    fn hoists_repeated_inline_link() {
        // Acceptance case (a): two occurrences -> two `[A]` + one definition.
        let input = "see [A](http://x) and [A](http://x)\n";
        let expected = "see [A] and [A]\n[A]: http://x\n";
        let out = fix_links(input);
        assert_eq!(&*out, expected, "repeated inline link should be hoisted");
    }

    #[test]
    fn single_occurrence_untouched_and_borrowed() {
        // Acceptance case (b): a link used once is left inline and borrowed.
        let input = "only [A](http://x) once\n";
        let out = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)), "single link is borrowed");
        assert_eq!(&*out, input);
    }

    #[test]
    fn link_inside_code_fence_untouched() {
        // Acceptance case (c): links inside a fenced code block are not hoisted.
        let input = "\
text
```rust
[A](http://x) and [A](http://x)
```
after
";
        let out = fix_links(input);
        assert_eq!(&*out, input, "links inside code fences must not be hoisted");
    }

    #[test]
    fn autolink_and_whitespace_url_untouched() {
        // Acceptance case (d): `<...>` autolink and whitespace URLs are skipped.
        let input = "see [A](<http://x>) and [B](http://x y)\n";
        let out = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)), "non-inline forms borrowed");
        assert_eq!(&*out, input);
    }

    #[test]
    fn doc_comment_prefix_preserved() {
        // Acceptance case (f): a `///` prefix is preserved on rewritten links.
        let input = "/// see [A](http://x) and [A](http://x)\n";
        let expected = "/// see [A] and [A]\n[A]: http://x\n";
        let out = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn already_reference_style_is_borrowed() {
        // Acceptance case (g): re-running on reference-style output is a no-op.
        let input = "see [A] and [A]\n[A]: http://x\n";
        let out = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(&*out, input);
    }

    #[test]
    fn existing_definition_prevents_hoist() {
        // A pre-existing `[A]:` definition (any URL) excludes the pair, so the
        // inline occurrences are left as-is rather than re-targeted.
        let input = "[A](http://x) [A](http://x)\n[A]: http://z\n";
        let out = fix_links(input);
        assert_eq!(&*out, input);
    }

    #[test]
    fn same_text_different_url_hoists_first_only() {
        // Two pairs share text "A" with different URLs. To avoid emitting two
        // `[A]:` definitions, only the first-seen pair is hoisted; the second
        // pair stays inline.
        let input = "[A](http://x) [A](http://x) [A](http://y) [A](http://y)\n";
        let out = fix_links(input);
        let s = &*out;
        assert!(s.contains("[A]: http://x"), "first pair hoisted:\n{s}");
        assert!(
            !s.contains("[A]: http://y"),
            "second pair not re-defined:\n{s}"
        );
    }

    #[test]
    fn idempotent_on_hoisted_output() {
        let input = "see [A](http://x) and [A](http://x)\n";
        let once = fix_links(input).into_owned();
        let twice = fix_links(&once).into_owned();
        assert_eq!(twice, once, "fix_links must be idempotent");
    }

    #[test]
    fn optimized_is_idempotent_on_diverse_cases() {
        // Broad corpus: repeated vs single-use links, reference definitions,
        // autolinks, whitespace URLs, links inside code fences, doc-comment
        // prefixes, nested brackets, non-ASCII text, unbalanced edge cases.
        // `fix_links` must stay idempotent on every input.
        let cases: &[&str] = &[
            "",
            "no brackets at all\n",
            "single line, no trailing newline",
            "see [A](http://x) once and [B](http://y) once\n",
            "[ref]: http://x\n",
            "[A][] and [A][ref]\n",
            "see [A](http://x) and [A](http://x)\n",
            "[A](u) [A](u) [B](v) [B](v) [C](w)\n",
            "[A](http://x) [A](http://x) [A](http://y) [A](http://y)\n",
            "[A](http://x) [A](http://x)\n[A]: http://z\n",
            "see [A](<http://x>) and [B](http://x y)\n",
            "[A]() is an empty url\n",
            "text\n```rust\n[A](u) and [A](u)\n```\nafter\n",
            "~~~\n[A](u) [A](u)\n~~~\n",
            "~~~text\n```rust\n[A](u) [A](u)\n```\n~~~\n",
            "/// see [A](u) and [A](u)\n",
            "//! [A](u) [A](u)\n",
            "/// [A](u) once only\n",
            "see [A] and [A]\n[A]: http://x\n",
            "[a [b] c](u) repeated [a [b] c](u)\n",
            "[[x]](u) and [[x]](u)\n",
            "[not a link\n",
            "[no](paren\n",
            "text [only] bracket\n",
            "a](b) without open\n",
            "[A]: http://a\n[B]: http://b\n[A](u) [A](u)\n",
            "café [A](u) déjà [A](u) vu\n",
            "emoji 😀 [A](u) 🚀 [A](u)\n",
            "/// 日本語 [A](u) and [A](u)\n",
            "[A](u) once then more [A](u) twice and [A](u) twice\n",
            "see [A](u) and [A](u)",
            "see [A](u) and [A](u)\r\n",
            "    /// [A](u) [A](u)\n",
        ];
        for &input in cases {
            let once = fix_links(input).into_owned();
            let twice = fix_links(&once).into_owned();
            assert_eq!(twice, once, "not idempotent for input {input:?}");
        }
    }
}
