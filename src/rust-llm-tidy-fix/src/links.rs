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

use crate::fences::parse_fence;
use crate::tables::{split_terminator, strip_doc_prefix};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

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

/// Update the open-fence stack for the (doc-prefix-stripped) line `body` and
/// report whether it is a fence delimiter line. Reuses the byte-exact
/// [`crate::fences::parse_fence`], so fence skipping stays in lock-step with
/// `fix_fences`.
///
/// `body` is the result of [`strip_doc_prefix`], so the `///` / `//!` marker
/// (and its indent) is already gone; only an optional inner indent may remain.
fn step_fence(stack: &mut Vec<(char, usize)>, body: &str) -> bool {
    // Cheap candidate check: after leading whitespace, a fence must start with
    // a backtick/tilde run. Non-ASCII-leading lines defer to the full Unicode
    // `trim_start` (sound superset gate, identical to `fix_fences`'s
    // `is_fence_candidate`), so typical code/prose lines skip the pipeline.
    if !is_fence_candidate_body(body) {
        return false;
    }
    let stripped = body.trim_start();
    let Some((marker, run_len, info)) = parse_fence(stripped) else {
        return false;
    };
    let is_closer = info.is_empty()
        && stack
            .last()
            .map(|(m, r)| *m == marker && *r <= run_len)
            .unwrap_or(false);
    if is_closer {
        stack.pop();
    } else {
        stack.push((marker, run_len));
    }
    true
}

/// Cheaply decide whether the doc-prefix-stripped `body` could begin a fence
/// under the Unicode `body.trim_start()` pipeline.
///
/// Sound superset gate (mirrors `fix_fences`'s `is_fence_candidate`): returns
/// `true` for every line the pipeline treats as a fence, plus a few extras the
/// pipeline emits verbatim. The common case - an ASCII line whose first
/// non-whitespace byte is not a marker run - short-circuits with a byte scan.
///
/// Whitespace in two tiers: ASCII whitespace (`0x09..=0x0d` plus `0x20`, the
/// ASCII members of [`char::is_whitespace`]) is skipped directly; a leading
/// non-ASCII byte (`>= 0x80`) may be Unicode whitespace before a fence, so such
/// lines defer to the full pipeline.
#[inline]
fn is_fence_candidate_body(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], 0x09..=0x0d | 0x20) {
        i += 1;
    }
    match bytes.get(i).copied() {
        // ASCII first byte: a fence - after the pipeline's Unicode trim - can
        // only start with a marker run.
        Some(b) if b <= 0x7f => b == b'`' || b == b'~',
        // Non-ASCII leading byte: may be Unicode whitespace before a fence;
        // defer to the full pipeline, which handles Unicode whitespace exactly.
        Some(_) => true,
        // Line was whitespace only (or empty): not a fence.
        None => false,
    }
}

/// If `body` is a reference-definition line (`[text]: url`), return the link
/// `text`. Otherwise return `None`.
#[inline]
fn definition_text(body: &str) -> Option<&str> {
    let s = body.trim_start();
    let after = s.strip_prefix('[')?;
    let close = after.find(']')?;
    let text = &after[..close];
    let rest = after[close + 1..].strip_prefix(':')?;
    // CommonMark requires whitespace (or end-of-line) after the colon.
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
        Some(text)
    } else {
        None
    }
}

/// Scan `body` for inline links `[text](url)` and tally each `(text, url)`,
/// recording first-seen order in `order`. Reference-style, autolink, and
/// whitespace-URL forms never match the inline shape, so they are skipped.
///
/// Jumps between `[` bytes with [`str::find`] instead of walking every
/// character: the cost is O(number of brackets), not O(text). `[` is ASCII, so
/// byte offsets are valid char boundaries and behavior is identical to a
/// char-by-char scan.
fn tally_links<'a>(
    body: &'a str,
    counts: &mut HashMap<(&'a str, &'a str), usize>,
    order: &mut Vec<(&'a str, &'a str)>,
) {
    let mut i = 0usize;
    while let Some(rel) = body[i..].find('[') {
        let open = i + rel;
        if let Some((text, url, end)) = parse_inline_link(body, open) {
            let prev = counts.get(&(text, url)).copied().unwrap_or(0);
            if prev == 0 {
                order.push((text, url));
            }
            counts.insert((text, url), prev + 1);
            i = end;
            continue;
        }
        // Not an inline link: step past this `[` (ASCII, so +1 is a char
        // boundary) and continue the search.
        i = open + 1;
    }
}

/// Rewrite eligible inline links in `body` to `[text]`, then re-attach `prefix`
/// and `term`. Returns `Some(new_segment)` if any link was rewritten, else
/// `None` (caller emits the original segment verbatim).
///
/// Output is allocated lazily: only once the first hoisted link is found. If
/// no link in `body` is hoisted, returns `None` with zero allocation. `last`
/// tracks how far the verbatim prefix of `body` has been emitted; non-hoisted
/// inline links leave `last` alone so their bytes are emitted verbatim in a
/// later gap (or the trailing copy), exactly like the eager version.
fn rewrite_links(
    prefix: &str,
    body: &str,
    term: &str,
    hoist: &HashSet<(&str, &str)>,
) -> Option<String> {
    let mut out: Option<String> = None;
    let mut last = 0usize;
    let mut i = 0usize;
    while let Some(rel) = body[i..].find('[') {
        let open = i + rel;
        match parse_inline_link(body, open) {
            Some((text, url, end)) if hoist.contains(&(text, url)) => {
                let o = out.get_or_insert_with(|| {
                    let mut s = String::with_capacity(prefix.len() + body.len() + term.len());
                    s.push_str(prefix);
                    s
                });
                o.push_str(&body[last..open]);
                o.push('[');
                o.push_str(text);
                o.push(']');
                last = end;
                i = end;
            }
            // Inline link but not hoisted: skip past it. `last` is unchanged so
            // its verbatim bytes are emitted in a later gap (or trailing copy).
            Some((_text, _url, end)) => i = end,
            // Lone `[` that is not an inline link: step one byte (ASCII
            // boundary) and continue the search.
            None => i = open + 1,
        }
    }
    let mut o = out?;
    o.push_str(&body[last..]);
    o.push_str(term);
    Some(o)
}

/// If `body` at byte index `open` (`[`) opens an inline link `[text](url)`,
/// return `(text, url, end)` where `end` is one past the closing `)`.
/// Returns `None` for reference-style forms, autolink `<...>` URLs, URLs
/// containing whitespace/newline, or unbalanced brackets.
#[inline]
fn parse_inline_link(body: &str, open: usize) -> Option<(&str, &str, usize)> {
    let bytes = body.as_bytes();
    // Walk to the matching `]`, allowing balanced nested `[ ]`.
    let mut depth = 1usize;
    let mut j = open + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if depth != 0 || j >= bytes.len() {
        return None;
    }
    let close_bracket = j;
    let text = &body[open + 1..close_bracket];
    // Require `(` immediately after `]`.
    let paren_open = close_bracket + 1;
    if paren_open >= bytes.len() || bytes[paren_open] != b'(' {
        return None;
    }
    // Walk to `)`, rejecting whitespace/newline inside the URL.
    let mut k = paren_open + 1;
    while k < bytes.len() && bytes[k] != b')' {
        if matches!(bytes[k], b' ' | b'\t' | b'\n' | b'\r') {
            return None;
        }
        k += 1;
    }
    if k >= bytes.len() {
        return None;
    }
    let url = &body[paren_open + 1..k];
    if url.is_empty() || url.starts_with('<') {
        return None;
    }
    Some((text, url, k + 1))
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

/// Append hoisted `[text]: url` definitions at the end of `buf`, each on its
/// own line. Ensures `buf` ends with a newline so the first definition starts
/// on its own line; if the document already ends with a reference definition
/// the new definitions continue that block contiguously.
fn append_definitions(buf: &mut String, hoist: &[(&str, &str)]) {
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
    for &(text, url) in hoist {
        buf.push('[');
        buf.push_str(text);
        buf.push_str("]: ");
        buf.push_str(url);
        buf.push('\n');
    }
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
