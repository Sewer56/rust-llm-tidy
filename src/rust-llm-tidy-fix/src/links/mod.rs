//! Collapse inline links to reference form in Markdown and Rust doc comments.
//!
//! [`fix_links`] scans `input` for inline links `[text](url)`, rewrites every
//! eligible occurrence to the reference form `[text]`, and records a
//! `[text]: url` definition. The default threshold is 1: every eligible inline
//! link hoists, including a single use and intra-doc `Self::…` / `crate::…`
//! targets.
//!
//! A `(text, url)` pair is eligible when it appears at least `min_occurrences`
//! times (default 1) in the non-fenced input and its text has no existing
//! `[text]:` definition. Skipped forms are unchanged:
//! autolinks (`<...>`), whitespace/newline-URL forms, already-reference-style
//! links (`[text][ref]`, `[text][]`, `[text]`), links whose text already has a
//! definition, and links inside fenced code blocks.
//!
//! Definitions are placed by content, not by file type. When the input carries
//! `///` or `//!` doc-comment lines outside code fences (Rust context), each
//! `[text]: url` definition is written at the end of every doc-comment block
//! that uses the label, on new lines with that block's doc prefix; links on
//! non-doc-comment lines are left alone. Otherwise (Markdown), one
//! document-scoped trailing definition block is appended at the end of the
//! input, separated from a trailing paragraph by a blank line so the
//! definitions parse as definitions (CommonMark forbids a link reference
//! definition from interrupting a paragraph). Definitions use the source's
//! dominant line ending.
//!
//! The function is idempotent and returns a borrowed [`Cow`] with empty pairs
//! when nothing is eligible.
//!
//! # Performance
//!
//! The default threshold-one path first rejects input without the exact `](`
//! inline-link shape, then parses each eligible occurrence once. Newlines use
//! vectorized `memchr`; small candidate/span sets stay inline; rewriting copies
//! directly between saved spans with exact output capacity. Reference-only and
//! link-free input returns [`Cow::Borrowed`] without allocation.
//!
//! # Example
//!
//! ```rust
//! # use std::borrow::Cow;
//! use rust_llm_tidy_fix::fix_links;
//!
//! // Markdown: a single use still hoists, with a trailing definition.
//! let input = "see [A](http://x)\n";
//! let expected = "see [A]\n\n[A]: http://x\n";
//! let (out, pairs) = fix_links(input);
//! assert_eq!(out.into_owned(), expected);
//! assert_eq!(pairs, [("[A](http://x)".to_string(), "[A]".to_string())]);
//! assert!(matches!(fix_links(expected).0, Cow::Borrowed(_)));
//! ```

use crate::tables::{split_terminator, strip_doc_prefix};
use rewrite::{
    append_block_definitions, append_definitions, dominant_line_ending, replacement_pair,
    rewrite_links, rewrite_links_track, tally_links,
};
use scan::{definition_text, doc_block_key, line_segments, step_fence};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

mod fast;
mod rewrite;
mod scan;

/// Collapse eligible inline links `[text](url)` to reference form.
///
/// Always-hoist contract (threshold 1): every eligible link, single-use and
/// intra-doc included, becomes `[text]` plus a `[text]: url` definition.
/// Returns the rewritten text and one `(before, after)` substitution per
/// hoisted link; borrows `input` back with no pairs when nothing is eligible.
///
/// # Arguments
///
/// - `input` - the Markdown or Rust source to rewrite.
pub fn fix_links(input: &str) -> (Cow<'_, str>, Vec<(String, String)>) {
    fast::fix_links_one(input)
}

/// Collapse each inline link `[text](url)` to reference form when its
/// `(text, url)` pair appears at least `min_occurrences` times in the
/// non-fenced input.
///
/// `min_occurrences = 1` reproduces [`fix_links`] exactly. Returns the rewritten
/// text plus one `(before, after)` substitution per hoisted link, borrowing
/// `input` back with no pairs when nothing is eligible.
///
/// # Arguments
///
/// - `input` - the Markdown or Rust source to rewrite.
/// - `min_occurrences` - how often a `(text, url)` pair must occur to hoist;
///   values below 1 are treated as 1.
pub fn fix_links_with_min(
    input: &str,
    min_occurrences: usize,
) -> (Cow<'_, str>, Vec<(String, String)>) {
    if min_occurrences <= 1 {
        return fast::fix_links_one(input);
    }

    fix_links_counted(input, min_occurrences)
}

/// Counted implementation used for configurable thresholds above one and as a
/// differential correctness oracle for the specialized threshold-one engine.
fn fix_links_counted(input: &str, min_occurrences: usize) -> (Cow<'_, str>, Vec<(String, String)>) {
    // Fast path: no link-opening bracket means nothing can change.
    if !input.contains('[') {
        return (Cow::Borrowed(input), Vec::new());
    }

    // Pass 1: tally eligible inline links (outside code fences), record the
    // texts of every existing `[text]:` definition so we never re-define one,
    // and detect whether the input is Rust doc-comment content. Splitting each
    // line once (into content/body) feeds the fence step, the link tally, and
    // the Rust-context detection, and the `contains('[')` guard skips link
    // work for the common bracket-less line.
    let mut fence_stack: Vec<(char, usize)> = Vec::new();
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    let mut order: Vec<(&str, &str)> = Vec::new();
    let mut existing: HashSet<&str> = HashSet::new();
    let mut rust_context = false;
    for (_, segment) in line_segments(input) {
        let (content, _) = split_terminator(segment);
        let (prefix, body) = strip_doc_prefix(content);
        let fence_delim = step_fence(&mut fence_stack, body);
        // Content-based Rust-context detection: a doc-comment line outside a
        // code fence (including a fence delimiter that itself carries `///`).
        if !prefix.is_empty() && (fence_delim || fence_stack.is_empty()) {
            rust_context = true;
        }
        if fence_delim || !fence_stack.is_empty() {
            continue;
        }
        if !body.contains('[') {
            continue;
        }
        if let Some(key) = definition_text(body) {
            existing.insert(key);
        }
        tally_links(body, &mut counts, &mut order);
    }

    // Hoist set: pairs seen at least `min_occurrences` times whose text is not
    // already defined. `existing.insert(text)` returns false for pre-existing
    // definitions and also dedups by text, so we never emit two `[text]:` lines
    // for one text and the first-seen `(text, url)` for a repeated text wins.
    let mut hoist: Vec<(&str, &str)> = Vec::new();
    let mut hoist_set: HashSet<(&str, &str)> = HashSet::new();
    for &(text, url) in &order {
        if counts[&(text, url)] >= min_occurrences && existing.insert(text) {
            hoist_set.insert((text, url));
            hoist.push((text, url));
        }
    }

    if hoist.is_empty() {
        return (Cow::Borrowed(input), Vec::new());
    }

    let le = dominant_line_ending(input);
    if rust_context {
        rewrite_rust_context(input, &hoist_set, le)
    } else {
        rewrite_markdown(input, &hoist_set, &hoist, le)
    }
}

/// Rewrite eligible inline links in Markdown context (no doc-comment lines),
/// appending one document-scoped trailing definition block at the end.
fn rewrite_markdown<'a>(
    input: &'a str,
    hoist_set: &HashSet<(&'a str, &'a str)>,
    hoist: &[(&str, &str)],
    le: &'a str,
) -> (Cow<'a, str>, Vec<(String, String)>) {
    let mut out: Option<String> = None;
    let mut fence_stack: Vec<(char, usize)> = Vec::new();
    for (seg_start, segment) in line_segments(input) {
        let (content, term) = split_terminator(segment);
        let (_, body) = strip_doc_prefix(content);
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
        match rewrite_links("", body, term, hoist_set) {
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
    // Definitions use the source's dominant line ending so a CRLF document
    // stays CRLF after hoisting.
    let mut buf = out.unwrap_or_else(|| {
        let mut s = String::with_capacity(input.len());
        s.push_str(input);
        s
    });
    append_definitions(&mut buf, hoist, le);

    // One `[text](url)` -> `[text]` pair per hoisted link, in hoist order.
    let pairs = hoist
        .iter()
        .map(|&(text, url)| replacement_pair(text, url))
        .collect();
    (Cow::Owned(buf), pairs)
}

/// Rewrite eligible inline links in Rust doc-comment context.
///
/// Hoisted links inside a `///` / `//!` block become `[text]`; a `[text]: url`
/// definition is appended to every block that uses the label. Links on
/// non-doc-comment lines are never rewritten or defined. Output is allocated
/// lazily, so an untouched input costs only the per-line `[` check.
fn rewrite_rust_context<'a>(
    input: &'a str,
    hoist_set: &HashSet<(&'a str, &'a str)>,
    le: &'a str,
) -> (Cow<'a, str>, Vec<(String, String)>) {
    // Lazily-allocated output; stays `None` when nothing actually changes.
    let mut out: Option<String> = None;
    let mut fence_stack: Vec<(char, usize)> = Vec::new();
    // The doc-comment block currently being emitted, its prefix, and its
    // hoisted pairs (defined-when-flushed), tracked for O(1) dedup.
    let mut cur_block: Option<&str> = None;
    let mut cur_defs: Vec<(&str, &str)> = Vec::new();
    let mut cur_defs_seen: HashSet<(&str, &str)> = HashSet::new();
    let mut hoisted: Vec<(&str, &str)> = Vec::new();
    let mut hoisted_seen: HashSet<(&str, &str)> = HashSet::new();

    for (seg_start, segment) in line_segments(input) {
        let (content, term) = split_terminator(segment);
        let (prefix, body) = strip_doc_prefix(content);
        let block_key = doc_block_key(prefix);

        // Close the previous block the moment the line leaves it (a new
        // block, a non-doc line, or a fence carries a different key), so each
        // using comment gets exactly one in-comment definition copy.
        if block_key != cur_block {
            flush_block_defs(&mut out, cur_block, &mut cur_defs, &mut cur_defs_seen, le);
            cur_block = block_key;
        }

        let fence_delim = step_fence(&mut fence_stack, body);
        if fence_delim || !fence_stack.is_empty() || prefix.is_empty() || !body.contains('[') {
            // Fence line, fenced line, or non-doc-comment line: never rewritten.
            if let Some(o) = out.as_mut() {
                o.push_str(segment);
            }
            continue;
        }

        // A non-fenced doc-comment line with a link: hoist eligible pairs,
        // collecting the ones used in this block for its definition lines.
        match rewrite_links_track(prefix, body, term, hoist_set, |t, u| {
            if cur_defs_seen.insert((t, u)) {
                cur_defs.push((t, u));
            }
            if hoisted_seen.insert((t, u)) {
                hoisted.push((t, u));
            }
        }) {
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
    flush_block_defs(&mut out, cur_block, &mut cur_defs, &mut cur_defs_seen, le);

    if hoisted.is_empty() {
        return (Cow::Borrowed(input), Vec::new());
    }

    let pairs = hoisted
        .iter()
        .map(|&(text, url)| replacement_pair(text, url))
        .collect();
    (
        Cow::Owned(out.expect("hoisted pairs imply changed output")),
        pairs,
    )
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

/// Append a doc-comment block's collected definition lines to `out` and reset
/// its accumulator (with the dedup set), so the next block starts fresh.
/// No-op when the block rewrote nothing.
fn flush_block_defs(
    out: &mut Option<String>,
    prefix: Option<&str>,
    defs: &mut Vec<(&str, &str)>,
    seen: &mut HashSet<(&str, &str)>,
    le: &str,
) {
    if defs.is_empty() {
        return;
    }
    let prefix = prefix.expect("definitions imply an open doc-comment block");
    let o = out.get_or_insert_with(String::new);
    append_block_definitions(o, prefix, defs, le);
    defs.clear();
    seen.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn no_bracket_returns_borrowed() {
        let input = "hello world\nno links here\n";
        let (out, pairs) = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)), "no link -> borrowed");
        assert!(pairs.is_empty());
        assert_eq!(&*out, input);
    }

    #[test]
    fn hoists_repeated_inline_link() {
        // Acceptance case (a): two occurrences -> two `[A]` + one definition.
        let input = "see [A](http://x) and [A](http://x)\n";
        let expected = "see [A] and [A]\n\n[A]: http://x\n";
        let (out, pairs) = fix_links(input);
        assert_eq!(&*out, expected, "repeated inline link should be hoisted");
        assert_eq!(pairs, [("[A](http://x)".into(), "[A]".into())]);
    }

    #[test]
    fn single_occurrence_markdown_hoists() {
        // Acceptance case (b): a link used once in Markdown is still hoisted
        // by default, gaining a trailing definition.
        let input = "only [A](http://x) once\n";
        let expected = "only [A] once\n\n[A]: http://x\n";
        let (out, pairs) = fix_links(input);
        assert_eq!(&*out, expected, "single Markdown link should hoist");
        assert_eq!(pairs, [("[A](http://x)".into(), "[A]".into())]);
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
        let (out, _) = fix_links(input);
        assert_eq!(&*out, input, "links inside code fences must not be hoisted");
    }

    #[test]
    fn mismatched_marker_inside_fence_is_content() {
        // A `~~~` line inside a backtick fence is code-block content, not a
        // second opener, so the `` ``` `` closer really closes the block and the
        // link after it is still hoisted.
        let input = "```text\n~~~\n```\nsee [A](http://x) here\n";
        let expected = "```text\n~~~\n```\nsee [A] here\n\n[A]: http://x\n";
        let (out, pairs) = fix_links(input);
        assert_eq!(
            &*out, expected,
            "fence must close despite the inner `~~~` line"
        );
        assert_eq!(pairs, [("[A](http://x)".into(), "[A]".into())]);
    }

    #[test]
    fn mismatched_marker_inside_doc_fence_is_content() {
        // Same case inside a `///` comment: the doc fence closes and the link
        // after it hoists with an in-comment definition.
        let input = "/// ```text\n/// ~~~\n/// ```\n/// see [A](http://x) here\n";
        let expected = "/// ```text\n/// ~~~\n/// ```\n/// see [A] here\n///\n/// [A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn short_same_marker_run_inside_fence_is_content() {
        // A shorter run of the opener's marker cannot close it, and must not
        // open a nested block either; the 4-backtick closer still closes.
        let input = "````text\n```\nsee [A](http://x) inside\n````\nsee [B](http://y) after\n";
        let expected =
            "````text\n```\nsee [A](http://x) inside\n````\nsee [B] after\n\n[B]: http://y\n";
        let (out, pairs) = fix_links(input);
        assert_eq!(
            &*out, expected,
            "only the link outside the fence should hoist"
        );
        assert_eq!(pairs, [("[B](http://y)".into(), "[B]".into())]);
    }

    #[test]
    fn autolink_and_whitespace_url_untouched() {
        // Acceptance case (d): `<...>` autolink and whitespace URLs are skipped.
        let input = "see [A](<http://x>) and [B](http://x y)\n";
        let (out, pairs) = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)), "non-inline forms borrowed");
        assert!(pairs.is_empty());
        assert_eq!(&*out, input);
    }

    #[test]
    fn doc_comment_prefix_preserved() {
        // Acceptance case (f): the `///` prefix is preserved on rewritten links
        // and the definition lands inside the comment, on the same prefix.
        let input = "/// see [A](http://x) and [A](http://x)\n";
        let expected = "/// see [A] and [A]\n///\n/// [A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn already_reference_style_is_borrowed() {
        // Acceptance case (g): re-running on reference-style output is a no-op.
        let input = "see [A] and [A]\n\n[A]: http://x\n";
        let (out, pairs) = fix_links(input);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert!(pairs.is_empty());
        assert_eq!(&*out, input);
    }

    #[test]
    fn existing_definition_prevents_hoist() {
        // A pre-existing `[A]:` definition (any URL) excludes the pair, so the
        // inline occurrences are left as-is rather than re-targeted.
        let input = "[A](http://x) [A](http://x)\n[A]: http://z\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, input);
    }

    #[test]
    fn same_text_different_url_hoists_first_only() {
        // Two pairs share text "A" with different URLs. To avoid emitting two
        // `[A]:` definitions, only the first-seen pair is hoisted; the second
        // pair stays inline.
        let input = "[A](http://x) [A](http://x) [A](http://y) [A](http://y)\n";
        let (out, pairs) = fix_links(input);
        let s = &*out;
        assert!(s.contains("[A]: http://x"), "first pair hoisted:\n{s}");
        assert!(
            !s.contains("[A]: http://y"),
            "second pair not re-defined:\n{s}"
        );
        assert!(
            s.contains("[A](http://y)"),
            "second pair stays inline:\n{s}"
        );
        assert_eq!(pairs, [("[A](http://x)".into(), "[A]".into())]);
    }

    #[test]
    fn same_text_different_url_in_doc_block_hoists_first_only() {
        // In Rust context, the first-seen URL in a comment wins: the `http://x`
        // pair collapses, the conflicting `http://y` uses stay inline.
        let input = "/// [A](http://x) [A](http://x) [A](http://y) [A](http://y)\n";
        let expected = "/// [A] [A] [A](http://y) [A](http://y)\n///\n/// [A]: http://x\n";
        let (out, pairs) = fix_links(input);
        assert_eq!(&*out, expected);
        assert_eq!(pairs, [("[A](http://x)".into(), "[A]".into())]);
    }

    #[test]
    fn multiple_hoisted_pairs_rewrite_each_line() {
        // Two distinct repeated pairs on different lines are both hoisted.
        let input = "see [A](u) and [A](u)\nthen [B](v) again [B](v)\n";
        let (out, pairs) = fix_links(input);
        let s = &*out;
        assert!(s.contains("see [A] and [A]"), "A hoisted:\n{s}");
        assert!(s.contains("then [B] again [B]"), "B hoisted:\n{s}");
        assert!(s.contains("[A]: u"), "A definition:\n{s}");
        assert!(s.contains("[B]: v"), "B definition:\n{s}");
        assert_eq!(
            pairs,
            [
                ("[A](u)".into(), "[A]".into()),
                ("[B](v)".into(), "[B]".into())
            ]
        );
    }

    #[test]
    fn multi_comment_repro_duplicates_defs_per_comment() {
        // The reported repro: the same intra-doc pairs in two separate `///`
        // comments. Each comment keeps its own `[text]` uses plus its own
        // duplicated definition lines; no definition appears at EOF or between
        // comments, so every comment stays rustdoc-clean.
        let input = "\
/// See [field](Self::field) and [path](crate::path).
pub struct S;

/// [field](Self::field) again and [path](crate::path).
impl S {}
";
        let expected = "\
/// See [field] and [path].
///
/// [field]: Self::field
/// [path]: crate::path
pub struct S;

/// [field] again and [path].
///
/// [field]: Self::field
/// [path]: crate::path
impl S {}
";
        let (out, pairs) = fix_links(input);
        assert_eq!(&*out, expected);
        assert_eq!(
            pairs,
            [
                ("[field](Self::field)".into(), "[field]".into()),
                ("[path](crate::path)".into(), "[path]".into())
            ]
        );
    }

    #[test]
    fn intra_doc_target_forms_hoist() {
        // Self::, super::, self::, crate::, bare identifiers and qualified
        // paths all hoist on the same terms as plain URLs, each with an
        // in-comment definition.
        let input = "/// [a](self::a) [b](super::b) [c](crate::c) [d](d) [e](foo::Bar::method)\n";
        let expected = "\
/// [a] [b] [c] [d] [e]
///
/// [a]: self::a
/// [b]: super::b
/// [c]: crate::c
/// [d]: d
/// [e]: foo::Bar::method
";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn single_use_in_one_comment_hoists() {
        // A link used once in a single `///` comment still hoists by default,
        // gaining its in-comment definition.
        let input = "/// see [A](http://x) once\n";
        let expected = "/// see [A] once\n///\n/// [A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn doc_comment_no_trailing_newline_hoists_on_own_line() {
        // A rewritten doc-comment line at EOF without a trailing newline still
        // receives its definition on a separate comment line, never glued onto
        // the rewritten line.
        let input = "/// see [A](http://x) and [A](http://x)";
        let expected = "/// see [A] and [A]\n///\n/// [A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn single_use_in_two_comments_duplicates_def() {
        // A link used once in each of two separate comments: both comments are
        // rewritten and each gets its own in-comment definition copy.
        let input = "\
/// first [A](http://x)
pub fn a() {}

/// again [A](http://x)
pub fn b() {}
";
        let expected = "\
/// first [A]
///
/// [A]: http://x
pub fn a() {}

/// again [A]
///
/// [A]: http://x
pub fn b() {}
";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn inner_doc_comment_parity() {
        // `//!` inner doc comments are handled exactly like `///` blocks.
        let input = "//! see [A](http://x) once\n";
        let expected = "//! see [A] once\n//!\n//! [A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected);
    }

    #[test]
    fn non_doc_commented_link_not_rewritten_and_borrowed() {
        // Rust context (one `///` line exists), but the only inline link sits
        // on a non-doc-comment line (a string literal): it is never rewritten
        // and gets no definition, so the pass returns the input borrowed.
        let input = "\
/// some doc
pub fn f() {
    let s = \"see [A](http://x)\";
}
";
        let (out, pairs) = fix_links(input);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "link on a non-doc line must not change the input"
        );
        assert!(pairs.is_empty());
        assert_eq!(&*out, input);
    }

    #[test]
    fn markdown_defs_separated_from_trailing_paragraph() {
        // A definition cannot interrupt a paragraph, so a document ending in
        // paragraph text gets one blank line before the definition block.
        let input = "see [A](http://x) and [A](http://x)\ntext\n";
        let expected = "see [A] and [A]\ntext\n\n[A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected, "blank line must separate defs");
    }

    #[test]
    fn markdown_defs_after_trailing_blank_line_add_no_extra() {
        // A document already ending in a blank line keeps exactly that one
        // separator; no second blank line is inserted.
        let input = "see [A](http://x) and [A](http://x)\n\n";
        let expected = "see [A] and [A]\n\n[A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected, "existing blank line must not double");
    }

    #[test]
    fn markdown_defs_continue_existing_definition_block() {
        // A document ending in a complete reference definition appends the new
        // definitions contiguously, with no blank line between them.
        let input = "see [A](http://x) and [A](http://x)\n\n[B]: http://y\n";
        let expected = "see [A] and [A]\n\n[B]: http://y\n[A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected, "definitions must stay contiguous");
    }

    #[test]
    fn definition_shaped_trailing_line_still_gets_blank() {
        // `[x]:` (no destination) and `[x]: junk` (trailing junk after the
        // destination, no valid title) are paragraph text, not definitions, so
        // appended definitions need a blank separator after them.
        for bad in ["[x]:", "[x]: not a valid dest title junk"] {
            let input = format!("see [A](http://x) and [A](http://x)\n{bad}\n");
            let (out, _) = fix_links(&input);
            let expected = format!("see [A] and [A]\n{bad}\n\n[A]: http://x\n");
            assert_eq!(&*out, expected, "must separate after pseudo-def {bad:?}");
        }
    }

    #[test]
    fn definition_with_title_counts_as_definition() {
        // A complete definition carrying a title keeps the contiguous append.
        let input = "see [A](http://x) and [A](http://x)\n\n[B]: http://y \"t\"\n";
        let expected = "see [A] and [A]\n\n[B]: http://y \"t\"\n[A]: http://x\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected, "titled definition stays contiguous");
    }

    #[test]
    fn crlf_markdown_defs_get_crlf_blank_separator() {
        // The blank separator uses the source's dominant line ending: every
        // `\n` in the output stays part of a `\r\n`.
        let input = "see [A](http://x) and [A](http://x)\r\ntext\r\n";
        let (out, _) = fix_links(input);
        let s = out.into_owned();
        assert!(
            s.contains("text\r\n\r\n[A]: http://x\r\n"),
            "CRLF blank separator expected: {s:?}"
        );
    }

    #[test]
    fn idempotent_on_hoisted_output() {
        let input = "see [A](http://x) and [A](http://x)\n";
        let once = fix_links(input).0.into_owned();
        let twice = fix_links(&once).0.into_owned();
        assert_eq!(twice, once, "fix_links must be idempotent");
    }

    #[test]
    fn optimized_is_idempotent_on_diverse_cases() {
        // Broad corpus: repeated vs single-use links, reference definitions,
        // autolinks, whitespace URLs, links inside code fences, doc-comment
        // prefixes, intra-doc forms, nested brackets, non-ASCII text,
        // unbalanced edge cases, and multi-comment Rust inputs. `fix_links`
        // must stay idempotent on every input.
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
            "```text\n~~~\n```\n[A](u) after\n",
            "````text\n```\n[A](u) inside\n````\n[B](v) after\n",
            "/// ```text\n/// ~~~\n/// ```\n/// [A](u) after\n",
            "/// see [A](u) and [A](u)\n",
            "//! [A](u) [A](u)\n",
            "/// [A](u) once only\n",
            "see [A] and [A]\n\n[A]: http://x\n",
            "[a [b] c](u) repeated [a [b] c](u)\n",
            "[[x]](u) and [[x]](u)\n",
            "[not a link\n",
            "[no](paren\n",
            "text [only] bracket\n",
            "a](b) without open\n",
            "[A]: http://a\n[B]: http://b\n[A](u) [A](u)\n",
            "[A]: http://a(junk\n[A](u) [A](u)\n",
            "/// [A](u) [A](u)\n/// [A]: http://x(junk\n",
            "[B]: http://x(y)z \"ti(tle)\"\n[A](u) [A](u)\n",
            "café [A](u) déjà [A](u) vu\n",
            "emoji 😀 [A](u) 🚀 [A](u)\n",
            "/// 日本語 [A](u) and [A](u)\n",
            "[A](u) once then more [A](u) twice and [A](u) twice\n",
            "see [A](u) and [A](u)",
            "see [A](u) and [A](u)\r\n",
            "    /// [A](u) [A](u)\n",
            // Rust-context single-use and intra-doc cases.
            "/// [A](u) single use\n",
            "/// [A](u) and [A](u)",
            "/// [field](Self::field) and [path](crate::path)\n",
            "/// [a](self::a) [b](super::b)\n",
            "/// [A](http://x) [A](http://x) [A](http://y) [A](http://y)\n",
            "/// [A](u)\npub fn a() {}\n\n/// [A](u)\npub fn b() {}\n",
            "//! [A](u) single use\n",
            "/// some doc\nlet s = \"[A](u)\";\n",
        ];
        for &input in cases {
            let (fast_out, fast_pairs) = fix_links(input);
            let (counted_out, counted_pairs) = fix_links_counted(input, 1);
            assert_eq!(
                counted_out, fast_out,
                "specialized output differs from counted engine for {input:?}"
            );
            assert_eq!(
                counted_pairs, fast_pairs,
                "specialized pairs differ from counted engine for {input:?}"
            );
            let once = fast_out.into_owned();
            let twice = fix_links(&once).0.into_owned();
            assert_eq!(twice, once, "not idempotent for input {input:?}");
        }
    }

    #[test]
    fn malformed_definition_lines_do_not_block_hoist() {
        // Each line is definition-shaped but malformed: CommonMark leaves it
        // as paragraph text, so the label is free to hoist and the appended
        // definition needs a blank separator after it. `definition_text` and
        // `is_reference_definition` share one parser, so both reject these.
        // Labels need one non-space character; unescaped `[` can never be in
        // a label; an absent, unclosed, or unbalanced destination, an
        // unclosed angle form, and a title glued to an angle destination all
        // leave paragraph text. (`[A]:u` is valid: whitespace after the
        // colon is optional, and a bare destination may contain quotes.)
        let bad_lines = [
            "[A]:",
            "[A] : u",
            "[ ]: u",
            "[A[x]: u",
            "[A]: http://x(junk",
            "[A]: http://x((y) z",
            "[A]: <u",
            "[A]: <a<b>",
            "[A]: <u>\"t\"",
        ];
        for bad in bad_lines {
            let input = format!("see [A](http://x) and [A](http://x)\n{bad}\n");
            let (out, pairs) = fix_links(&input);
            let expected = format!("see [A] and [A]\n{bad}\n\n[A]: http://x\n");
            assert_eq!(
                &*out, expected,
                "malformed definition {bad:?} must not block hoisting"
            );
            assert_eq!(
                pairs,
                [("[A](http://x)".into(), "[A]".into())],
                "pairs for {bad:?}"
            );
        }
    }

    #[test]
    fn complete_definition_forms_stay_contiguous() {
        // Complete CommonMark definitions: the trailing append stays
        // contiguous (no blank separator) for every accepted form.
        let good_lines = [
            "[B]: http://y",
            "[B]:http://y",
            "[B]: http://x(y)z",
            "[B]: http://x\\(y\\)",
            "[B]: <http://x>",
            "[B]: <u\\>v>",
            "[B]: http://y \"t\"",
            "[B]: http://y 't'",
            "[B]: http://y (t)",
            "[B]: http://y (t (u))",
        ];
        for good in good_lines {
            let input = format!("see [A](http://x) and [A](http://x)\n\n{good}\n");
            let (out, _) = fix_links(&input);
            let expected = format!("see [A] and [A]\n\n{good}\n[A]: http://x\n");
            assert_eq!(
                &*out, expected,
                "valid definition {good:?} must stay contiguous"
            );
        }
    }

    #[test]
    fn doc_block_malformed_def_line_gets_blank_separator() {
        // A malformed `[A]:`-shaped line at the end of a doc-comment block is
        // paragraph text: `needs_blank_before_defs` inserts the blank comment
        // line before the hoisted in-comment definition.
        for bad in ["[A]:", "[A]: http://x(junk", "[A]: <u>\"t\""] {
            let input = format!("/// see [A](http://x) and [A](http://x)\n/// {bad}\n");
            let (out, _) = fix_links(&input);
            let expected = format!("/// see [A] and [A]\n/// {bad}\n///\n/// [A]: http://x\n");
            assert_eq!(
                &*out, expected,
                "doc pseudo-definition {bad:?} must get a blank separator"
            );
        }
        // A complete definition at the block end stays contiguous.
        let input = "/// see [A](u) and [A](u)\n/// [B]: http://y\n";
        let expected = "/// see [A] and [A]\n/// [B]: http://y\n/// [A]: u\n";
        let (out, _) = fix_links(input);
        assert_eq!(&*out, expected, "complete doc definition stays contiguous");
    }

    #[test]
    fn crlf_definitions_use_crlf() {
        // CRLF input: the hoisted `[A]: http://x` definition must end with
        // `\r\n`, and every `\n` in the appended block is part of `\r\n`.
        let input = "see [A](http://x) and [A](http://x)\r\n";
        let (out, _) = fix_links(input);
        let s = out.into_owned();
        assert!(
            s.contains("[A]: http://x\r\n"),
            "hoisted definition must end with CRLF: {s:?}"
        );
        assert_eq!(
            s.matches('\n').count(),
            s.matches("\r\n").count(),
            "every newline must be CRLF: {s:?}"
        );
    }

    #[test]
    fn lf_definitions_use_lf() {
        // LF input: no `\r` should appear in the appended definition.
        let input = "see [A](http://x) and [A](http://x)\n";
        let (out, _) = fix_links(input);
        let s = out.into_owned();
        assert!(
            s.contains("[A]: http://x\n"),
            "hoisted definition must end with LF: {s:?}"
        );
        assert!(!s.contains('\r'), "no CR in LF output: {s:?}");
    }

    #[test]
    fn crlf_no_trailing_newline_uses_crlf_guard() {
        // CRLF input without a trailing newline: the `ends_with('\n')` guard
        // in `append_definitions` must push `le` ("\r\n") before the first
        // definition, and every `\n` in the output must be part of `\r\n`.
        let input = "intro\r\nsee [A](http://x) and [A](http://x)";
        let (out, _) = fix_links(input);
        let s = out.into_owned();
        assert!(
            s.contains("[A]: http://x\r\n"),
            "hoisted definition must end with CRLF: {s:?}"
        );
        assert_eq!(
            s.matches('\n').count(),
            s.matches("\r\n").count(),
            "every newline must be CRLF: {s:?}"
        );
    }

    #[test]
    fn idempotent_on_crlf_output() {
        // Re-running on CRLF reference-style output must stay CRLF (borrowed).
        let input = "see [A](http://x) and [A](http://x)\r\n";
        let once = fix_links(input).0.into_owned();
        let (twice, pairs) = fix_links(&once);
        assert!(
            matches!(twice, Cow::Borrowed(_)),
            "idempotent re-run must be borrowed"
        );
        assert!(pairs.is_empty(), "no hoist on idempotent re-run");
        assert_eq!(&*twice, &once, "idempotent on CRLF output");
        assert_eq!(
            once.matches('\n').count(),
            once.matches("\r\n").count(),
            "every newline must be CRLF: {once:?}"
        );
    }

    #[test]
    fn fix_links_with_min_one_matches_fix_links() {
        // A threshold of 1 must reproduce the default always-hoist contract
        // byte-for-byte on both Markdown and Rust doc-comment inputs.
        let cases: &[&str] = &[
            "see [A](http://x) and [A](http://x)\n",
            "only [A](http://x) once\n",
            "/// [field](Self::field) and [path](crate::path)\n",
            "/// see [A](http://x) once\n",
        ];
        for &input in cases {
            let (out, pairs) = fix_links(input);
            let (min_out, min_pairs) = fix_links_counted(input, 1);
            assert_eq!(
                min_out.into_owned(),
                out.into_owned(),
                "threshold 1 must match default for {input:?}"
            );
            assert_eq!(
                min_pairs, pairs,
                "threshold 1 must report the same pairs for {input:?}"
            );
        }
    }

    #[test]
    fn fix_links_with_min_two_keeps_single_use_inline() {
        // With a threshold of 2, a single-occurrence pair stays inline (no
        // record, input borrowed) in both Markdown and Rust doc-comment context.
        let markdown = "only [A](http://x) once\n";
        let (out, pairs) = fix_links_with_min(markdown, 2);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "single markdown use below threshold stays borrowed"
        );
        assert!(pairs.is_empty());
        assert_eq!(&*out, markdown);

        let rust = "/// see [A](http://x) once\n";
        let (out, pairs) = fix_links_with_min(rust, 2);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "single doc-comment use below threshold stays borrowed"
        );
        assert!(pairs.is_empty());
        assert_eq!(&*out, rust);
    }

    #[test]
    fn fix_links_with_min_two_still_hoists_repeated_pair() {
        // A threshold of 2 must still hoist a pair that appears twice.
        let input = "see [A](http://x) and [A](http://x)\n";
        let expected = "see [A] and [A]\n\n[A]: http://x\n";
        let (out, pairs) = fix_links_with_min(input, 2);
        assert_eq!(out.into_owned(), expected);
        assert_eq!(pairs, [("[A](http://x)".into(), "[A]".into())]);
    }
}
