//! Parses Rust items with spans, attached comments, and preamble/trailer offsets.
//!
//! This module orchestrates parsing. It splits source text into top-level
//! items with byte spans (comment-pinning prefix comments/attributes to each
//! item), classifies them, and exposes the data model.
//!
//! Shared item types live in [`crate::source`].
//!
//! Parsing is performed with tree-sitter (the `tree-sitter-rust` grammar),
//! which yields byte offsets directly - no line/column conversion is needed.
//!
//! # Children
//!
//! - `classify`: node classification feeding this orchestration; its own
//!   child map splits trivia handling from signature facts.
//!
//! # Re-exports
//!
//! - `doc_attribute_content`, `is_outer_doc`: doc reads shared with the
//!   language module's doc-region producer.

use self::classify::classify_item;
use self::classify::result_error_type;
use self::classify::{PendingTrivia, is_attachable, is_transparent_comment};
pub(in crate::languages::rust) use self::classify::{doc_attribute_content, is_outer_doc};
use crate::source::{ParseResult, SourceItem};
use core::mem;

mod classify;

/// Kinds of `impl` members the DOC rules can check: methods, associated
/// consts, and associated types.
///
/// Other members, such as `use` inside an impl, are not documentable items.
const IMPL_MEMBER_KINDS: [&str; 3] = ["function_item", "const_item", "type_item"];

/// A raw top-level item entry: body node plus pending attachable trivia.
///
/// Details:
/// - Trivia is attributes plus outer doc comments.
/// - The body node is the wrapping `expression_statement` for a top-level
///   macro invocation.
struct RawEntry<'a> {
    /// Node whose byte range covers the item body (incl. trailing `;` for
    /// macro invocations wrapped in `expression_statement`).
    body: tree_sitter::Node<'a>,
    /// Attachable trivia immediately preceding the item.
    pending: PendingTrivia<'a>,
}

/// The items declared inside `impl` blocks: methods, associated consts,
/// and types.
///
/// Returns them as [`SourceItem`]s in source order, complementing the
/// top-level items in [`ParseResult`], which cover each `impl` as one
/// unit.
///
/// Each item's `start`/`end` byte offsets run from the member's leading
/// docs to its body end. They exist for diagnostics, not reordering.
/// Recomputes line starts from `source`; [`ParseResult`] retains none.
///
/// Skips function bodies and test modules. `#[cfg(test)]` impl members
/// stay checked, like `#[cfg(test)]` free fns; visibility-less members
/// (trait impls) come out private, which the rules skip.
pub(crate) fn impl_member_items(source: &str, tree: &tree_sitter::Tree) -> Vec<SourceItem> {
    let line_starts = line_start_offsets(source);
    let mut out = Vec::new();
    collect_impl_members(tree.root_node(), false, source, &line_starts, &mut out);
    out
}

/// Parse a Rust source file and extract items with spans.
///
/// Uses tree-sitter to parse the file and walk the syntax tree, extracting byte
/// spans for each top-level item. Comments and attributes that syntactically
/// precede an item are pinned to it (comment-pinning).
///
/// Spans are laid back-to-back so each item carries the blank lines and `//`
/// comments preceding it when reordered. Each item's `end` is the byte after
/// its trailing newline, and every non-first item's `start` is the previous
/// item's `end`.
///
/// The span mechanics live in `build_items`.
///
/// # Arguments
///
/// - `source`: the Rust source text to parse.
///
/// # Errors
///
/// Returns an error when tree-sitter cannot allocate a parse:
/// - `Parser::set_language` rejects the language (should not happen with the
///   bundled grammar).
/// - `Parser::parse` returns `None` (tree-sitter failed to produce a tree).
///
/// tree-sitter performs error recovery, so syntactically invalid Rust still
/// yields a tree (possibly with `ERROR` nodes) rather than a parse error.
pub(crate) fn parse_source(source: &str) -> anyhow::Result<ParseResult> {
    let lang = tree_sitter_rust::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter parse returned no tree"))?;

    let line_starts = line_start_offsets(source);
    let raw = collect_item_entries(tree.root_node());
    let items = build_items(&raw, source, &line_starts);

    let preamble_end = items.first().map(|it| it.start).unwrap_or(0);
    let trailer_start = items.last().map_or(source.len(), |last| last.end);

    Ok(ParseResult::new(
        items,
        source.to_string(),
        tree,
        preamble_end,
        trailer_start,
    ))
}

/// Assign each item a span that carries the blank lines and `//` comments
/// preceding it, so reordering preserves that whitespace.
///
/// Spans are laid back-to-back. Each item's `start` is the previous item's
/// `end` (`preamble_end` for the first), and each `end` is the byte after the
/// item's trailing newline.
///
/// Consecutive spans thus touch with no overlap, and the gap between two
/// items' bodies falls inside the second item's span. For:
///
/// ```text
/// fn a() {}
///
/// // header
/// fn b() {}
/// ```
///
/// item `a` ends right after its own newline; item `b` starts there, so
/// `source[b.start..b.end]` contains the blank line and `// header`.
///
/// `start_line` uses the item's attached-trivia start (the first preceding
/// `#[...]` attribute or `///` doc comment) so diagnostic line numbers point at
/// the real leading docs/attrs. With no attached trivia, it falls back to
/// the item body start.
fn build_items(raw: &[RawEntry<'_>], source: &str, line_starts: &[usize]) -> Vec<SourceItem> {
    let source_len = source.len();
    let mut out = Vec::with_capacity(raw.len());
    let mut prev_end: usize = 0;
    let mut first = true;

    for entry in raw {
        let body = entry.body;
        let body_start = body.start_byte();
        let body_end = body.end_byte();

        // attached_start = start of first preceding attr/outer-doc, else body.
        let attached_start = entry.pending.attached_start().unwrap_or(body_start);

        // Gap-anchored start: the first item seeds with its attached_start
        // (= preamble_end).
        //
        // Later items chain from the previous item's end so the inter-item
        // gap falls inside this item's span.
        let start = if first {
            first = false;
            attached_start
        } else {
            prev_end
        };

        // `end` extends body_end to the byte after its terminating newline:
        // the first line-start strictly greater than body_end.
        let end = next_line_start(line_starts, body_end).unwrap_or(source_len);

        out.push(item_from_class(
            body,
            &entry.pending,
            start,
            end,
            attached_start,
            line_starts,
            source,
        ));
        prev_end = end;
    }
    out
}

/// Descend `impl` bodies and non-test modules, collecting members into
/// `out`.
///
/// Only `impl` bodies contribute members (`in_impl_body`); elsewhere the
/// same kinds are top-level items the lint pass already checks, so
/// collecting them would duplicate diagnostics.
///
/// Trivia attachment mirrors [`collect_item_entries`]: attrs/outer docs
/// attach to the following member, and unrecognized nodes (e.g. `ERROR`
/// recovery) stay transparent.
fn collect_impl_members(
    container: tree_sitter::Node<'_>,
    in_impl_body: bool,
    source: &str,
    line_starts: &[usize],
    out: &mut Vec<SourceItem>,
) {
    let mut pending = PendingTrivia::new();
    let mut cursor = container.walk();
    for child in container.named_children(&mut cursor) {
        if is_attachable(child) {
            pending.push(child);
            continue;
        }
        if is_transparent_comment(child) {
            pending.note_comment(child);
            continue;
        }
        let is_member = in_impl_body && impl_member_entry_for(child).is_some();
        if is_member {
            // `start` is the first attached doc/attr so diagnostics point at
            // the real leading docs; `end` is the member body end.
            let body = child;
            let attached_start = pending.attached_start().unwrap_or(body.start_byte());
            out.push(item_from_class(
                body,
                &pending,
                attached_start,
                body.end_byte(),
                attached_start,
                line_starts,
                source,
            ));
        } else {
            match child.kind() {
                "impl_item" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        collect_impl_members(body, true, source, line_starts, out);
                    }
                }
                "mod_item" => {
                    // Skip test modules entirely; their helpers are exempt.
                    if !classify_item(child, source, &pending).is_test_module
                        && let Some(body) = child.child_by_field_name("body")
                    {
                        collect_impl_members(body, false, source, line_starts, out);
                    }
                }
                _ => {}
            }
        }
        // Only recognized items consume the pending run, exactly like
        // `collect_item_entries`; unrecognized nodes stay transparent.
        //
        // `item_entry_for` covers the member kinds as well, so the gate is
        // the same at file and impl-body level.
        if item_entry_for(child).is_some() {
            pending = PendingTrivia::new();
        }
    }
}

/// [`RawEntry`] per recognized top-level item.
///
/// Each entry carries the contiguous run of preceding attributes and outer
/// doc comments.
///
/// Non-attachable nodes (plain `//` comments, inner `//!` docs, empty
/// statements) are transparent: they neither attach to an item nor break a
/// pending run of attachable trivia.
fn collect_item_entries(root: tree_sitter::Node<'_>) -> Vec<RawEntry<'_>> {
    let mut entries = Vec::new();
    let mut pending = PendingTrivia::new();
    let count = root.named_child_count() as u32;
    for i in 0..count {
        let Some(child) = root.named_child(i) else {
            continue;
        };
        if is_attachable(child) {
            pending.push(child);
        } else if is_transparent_comment(child) {
            // Transparent: ignored for attachment, but a comment directly
            // above the item's attributes is its summary comment.
            pending.note_comment(child);
        } else if let Some(entry) = item_entry_for(child) {
            entries.push(RawEntry {
                body: entry,
                pending: mem::take(&mut pending),
            });
        } else {
            // Unrecognized non-item top-level node (e.g. a stray
            // `expression_statement` that is not a macro invocation).
            //
            // Treated as transparent so it does not break attachment of
            // surrounding trivia.
        }
    }
    entries
}

/// Byte offset of the start of every line in `source`.
///
/// `starts[0]` is always `0` (line 1 starts at offset 0). Each subsequent
/// entry is the byte offset immediately following a `'\n'`, i.e. the first
/// byte of the next line.
///
/// Built with a single SIMD-accelerated `memchr` scan.
fn line_start_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    // Heuristic preallocation: capacity = bytes/D; no regrowth when the file's
    // average bytes/line >= D. Measured across 3820 Rust files (~1.25M lines):
    //
    //   D=24 -> ~93% no regrow, D=21 -> ~96%, D=20 -> ~97%.
    // D=21 chosen for >95% target with margin (median file = ~33 bytes/line).
    let mut starts: Vec<usize> = Vec::with_capacity(bytes.len() / 21 + 1);
    starts.push(0);
    let mut from = 0;
    while let Some(pos) = memchr::memchr(b'\n', &bytes[from..]) {
        from += pos + 1;
        starts.push(from);
    }
    starts
}

/// If `node` is an impl member the DOC rules check, return the body node to
/// classify.
fn impl_member_entry_for(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    IMPL_MEMBER_KINDS.contains(&node.kind()).then_some(node)
}

/// If `node` is a recognized top-level item, return the body node to classify.
///
/// Top-level macro invocations are wrapped in `expression_statement`. The
/// body node returned is the `expression_statement` (so its byte range covers
/// the trailing `;`), with classification reading the inner `macro_invocation`.
fn item_entry_for(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match node.kind() {
        "function_item"
        | "struct_item"
        | "enum_item"
        | "union_item"
        | "type_item"
        | "impl_item"
        | "mod_item"
        | "trait_item"
        | "const_item"
        | "static_item"
        | "use_declaration"
        | "extern_crate_declaration"
        | "macro_definition"
        | "foreign_mod_item"
        | "macro_invocation" => Some(node),
        // A top-level `foo!();` parses as `expression_statement` wrapping a
        // `macro_invocation`.
        "expression_statement" => is_macro_invocation_stmt(node).then_some(node),
        _ => None,
    }
}

/// Classify `body` (reading `pending`) and build one [`SourceItem`] from
/// the result and its span facts.
///
/// Shared by the two walkers (`build_items` for top-level items,
/// `collect_impl_members` for impl members) so their construction cannot
/// drift apart.
///
/// `start`/`end` are the item span and differ per walker: gap-anchored
/// reorder spans at top level, diagnostic-only member spans inside impls.
///
/// `attached_start` (the first preceding attr/outer doc) anchors
/// `start_line`, and `pending` supplies the docs, the attributes behind
/// test-fn/test-module classification, and the summary-comment flag.
fn item_from_class(
    body: tree_sitter::Node<'_>,
    pending: &PendingTrivia<'_>,
    start: usize,
    end: usize,
    attached_start: usize,
    line_starts: &[usize],
    source: &str,
) -> SourceItem {
    let class = classify_item(body, source, pending);
    SourceItem::new(
        start,
        end,
        line_of(line_starts, attached_start),
        class.kind,
        class.name,
        class.impl_target,
        class.is_test_module,
        class.is_inline,
        class.is_trait_impl,
        class.visibility,
        class.doc_comments,
        class.returns_result,
        class.params,
        class.is_test_fn,
    )
    .with_result_error_type(result_error_type(body, source))
    .with_summary_comment(pending.has_summary_comment(body, source))
}

/// The first line-start strictly greater than `byte`, or `None` if none.
fn next_line_start(line_starts: &[usize], byte: usize) -> Option<usize> {
    // partition_point returns the count of starts <= byte, i.e. the index of
    // the first start strictly greater than byte.
    let idx = line_starts.partition_point(|&s| s <= byte);
    line_starts.get(idx).copied()
}

/// True when an `expression_statement`'s single named child is a
/// `macro_invocation`.
fn is_macro_invocation_stmt(stmt: tree_sitter::Node) -> bool {
    stmt.named_child_count() == 1
        && stmt
            .named_child(0)
            .is_some_and(|c| c.kind() == "macro_invocation")
}

/// 1-based line number of `byte` (count of line-starts at or before `byte`).
fn line_of(line_starts: &[usize], byte: usize) -> usize {
    line_starts.partition_point(|&s| s <= byte)
}

#[cfg(test)]
mod tests {
    use super::impl_member_items;
    use super::parse_source;
    use crate::source::ItemKind;
    use rstest::rstest;

    /// Gap-anchored spans: each non-first item's `start` is the previous
    /// item's `end`, `end` includes the trailing newline.
    ///
    /// `start_line` tracks the attached-trivia start (the SYN body start
    /// when no attached attrs/docs precede it).
    #[test]
    fn gap_anchored_spans_and_start_lines() {
        // Line map (1-based):
        //   1: //! doc
        //   2: fn b() {}
        //   3: (blank)
        //   4: // section header
        //   5: (blank)
        //   6: fn a() {}
        let source = "\
//! doc\n\
fn b() {}\n\
\n\
// section header\n\
\n\
fn a() {}\n";

        let parsed = parse_source(source).unwrap();

        // Two top-level items: b (line 2), a (line 6).
        assert_eq!(parsed.items.len(), 2, "two top-level items");

        // start_line uses the body start (no attached attrs/docs); the plain
        // `// section header` does not lower item a's start_line.
        assert_eq!(parsed.items[0].start_line(), 2, "item 0 on line 2");
        assert_eq!(parsed.items[1].start_line(), 6, "item 1 on line 6");

        // Gap-anchoring: item 1's start == item 0's end.
        assert_eq!(
            parsed.items[1].start, parsed.items[0].end,
            "item 1 start == item 0 end (gap-anchored)"
        );

        // Each item's end includes its trailing newline.
        let item0_body = &source[parsed.items[0].start..parsed.items[0].end];
        assert!(
            item0_body.ends_with('\n'),
            "item 0 end includes trailing \\n"
        );
        let item1_body = &source[parsed.items[1].start..parsed.items[1].end];
        assert!(
            item1_body.ends_with('\n'),
            "item 1 end includes trailing \\n"
        );

        // Item 1's slice carries the inter-item gap as leading trivia.
        assert!(
            item1_body.starts_with('\n'),
            "item 1 slice starts with carried gap (the \\n ending item 0's line)"
        );
        assert!(
            item1_body.contains("// section header"),
            "item 1 slice carries the // section header"
        );

        // Preamble is the module doc line.
        assert_eq!(&source[..parsed.preamble_end], "//! doc\n");

        // Trailer is empty (source ends right after the last item's newline).
        assert_eq!(parsed.trailer_start, source.len());
    }

    /// Last item without a trailing newline: `end` extends to `source.len()`.
    #[test]
    fn last_item_no_trailing_newline() {
        let source = "fn a() {}\nfn b() {}"; // no final \n
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(
            parsed.items[1].end,
            source.len(),
            "last item end == source.len() when no trailing newline"
        );
        // Trailer is empty.
        assert_eq!(parsed.trailer_start, source.len());
    }

    /// Single item: start == preamble_end, end extends through trailing newline.
    #[test]
    fn single_item() {
        let source = "//! doc\nfn main() {}\n";
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].start_line(), 2);
        assert_eq!(parsed.items[0].start, parsed.preamble_end);
        let body = &source[parsed.items[0].start..parsed.items[0].end];
        assert!(body.starts_with("fn main"));
        assert!(body.ends_with('\n'));
    }

    /// `#[cfg(test)]` attaches to the following `mod`, lowering its start to
    /// the attribute and marking it a test module.
    #[test]
    fn cfg_test_attaches_to_mod() {
        let source = "#[cfg(test)]\npub mod tests {}";
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert!(parsed.items[0].is_test_module());
        // Attached trivia lowers start_line to the attribute line.
        assert_eq!(parsed.items[0].start_line(), 1);
        assert_eq!(parsed.items[0].start, 0);
    }

    /// Outer `///` doc comments attach to the following fn.
    #[test]
    fn outer_doc_attaches_to_fn() {
        let source = "/// Does the thing.\npub fn thing() {}";
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].doc_comments(), &[" Does the thing."]);
        assert_eq!(parsed.items[0].start_line(), 1);
    }

    /// `is_inline()` is true for an inline `mod foo { ... }` definition (body
    /// present) and false for a file-based `mod foo;` declaration (no body).
    #[test]
    fn is_inline_distinguishes_mod_definition_from_declaration() {
        let source = "mod file_decl;\nmod inline_def {}\n";
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].kind(), &ItemKind::Mod);
        assert!(
            parsed.items[0].name() == Some("file_decl"),
            "first mod is file_decl"
        );
        assert!(!parsed.items[0].is_inline(), "mod x; is not inline");
        assert_eq!(parsed.items[1].kind(), &ItemKind::Mod);
        assert!(
            parsed.items[1].name() == Some("inline_def"),
            "second mod is inline_def"
        );
        assert!(parsed.items[1].is_inline(), "mod x with a body is inline");
    }

    /// A top-level macro invocation (`foo!();`) is classified as
    /// [`ItemKind::MacroInvocation`].
    #[test]
    fn top_level_macro_invocation() {
        let source = "println!(\"x\");\nmacro_rules! m { () => {}; }\n";
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].kind(), &ItemKind::MacroInvocation);
        assert_eq!(parsed.items[0].name(), Some("println"));
        assert_eq!(parsed.items[1].kind(), &ItemKind::Macro);
        assert_eq!(parsed.items[1].name(), Some("m"));
    }

    /// Test markers classify `#[test]`, `#[rstest]`, and `#[test_case]` in
    /// both bare and scoped spellings.
    #[rstest]
    #[case::bare_test("#[test]\nfn f() {}")]
    #[case::scoped_test("#[tokio::test]\nfn f() {}")]
    #[case::rstest("#[rstest]\nfn f() {}")]
    #[case::scoped_rstest("#[rstest::rstest]\nfn f() {}")]
    #[case::test_case("#[test_case]\nfn f() {}")]
    #[case::scoped_test_case("#[test_case::test_case]\nfn f() {}")]
    fn test_markers_classify_as_tests(#[case] source: &str) {
        let parsed = parse_source(source).unwrap();

        assert!(
            parsed.items[0].is_test_fn(),
            "marker should classify as a test: {source}"
        );
    }

    /// An attribute that is not a test marker does not classify as a test.
    #[test]
    fn non_test_attribute_is_not_a_test() {
        let parsed = parse_source("#[cfg(test)]\nfn f() {}").unwrap();

        assert!(!parsed.items[0].is_test_fn());
    }

    /// `has_summary_comment` accepts a comment directly above the attributes
    /// and rejects one missing, detached, or below them.
    #[rstest]
    #[case::plain_above("// Note.\n#[test]\nfn f() {}", true)]
    #[case::doc_above("/// Note.\n#[test]\nfn f() {}", true)]
    #[case::doc_blank_line("/// Note.\n\n#[test]\nfn f() {}", false)]
    #[case::doc_block_above("/** Note. */\n#[test]\nfn f() {}", true)]
    #[case::doc_block_blank_line("/** Note. */\n\n#[test]\nfn f() {}", false)]
    #[case::multi_line_doc_above("/// One.\n/// Two.\n#[test]\nfn f() {}", true)]
    #[case::nearer_plain_comment("/// Old.\n\n// Summary.\n#[test]\nfn f() {}", true)]
    #[case::block_above("/* Note. */\n#[test]\nfn f() {}", true)]
    #[case::block_blank_line("/* Note. */\n\n#[test]\nfn f() {}", false)]
    #[case::inner_doc_above("//! Note.\n#[test]\nfn f() {}", true)]
    #[case::module_doc_blank_line("//! Note.\n\n#[test]\nfn f() {}", false)]
    #[case::blank_line("// Note.\n\n#[test]\nfn f() {}", false)]
    #[case::below_attributes("#[test]\n// Note.\nfn f() {}", false)]
    #[case::comment_between_attributes("#[test]\n// Note.\n#[ignore]\nfn f() {}", false)]
    #[case::crlf_plain_above("// Note.\r\n#[test]\r\nfn f() {}", true)]
    #[case::crlf_blank_line("// Note.\r\n\r\n#[test]\r\nfn f() {}", false)]
    #[case::no_comment("#[test]\nfn f() {}", false)]
    fn summary_comment_reflects_comment_adjacency(#[case] source: &str, #[case] expected: bool) {
        let parsed = parse_source(source).unwrap();

        assert_eq!(parsed.items[0].has_summary_comment(), expected, "{source}");
    }

    // ── impl_member_items: impl-block members for the DOC rules ──

    /// Member collection covers pub methods, attaches their `///` docs, and
    /// reports the doc line as the diagnostic start line.
    #[test]
    fn impl_member_items_collects_documented_pub_methods() {
        let source = "//! doc\npub struct S;\nimpl S {\n    /// Docs.\n    pub fn a(&self, x: u32) -> u32 { x }\n    fn b(&self) {}\n}\n";
        let parsed = parse_source(source).unwrap();

        let members = impl_member_items(source, parsed.syntax_tree());

        // The private method classifies too; the rules skip it downstream.
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name().as_deref(), Some("a"));
        assert_eq!(members[0].doc_comments(), &[" Docs."]);
        assert_eq!(members[0].start_line(), 4);
        assert_eq!(members[0].params(), &["x"]);
        assert_eq!(members[1].name().as_deref(), Some("b"));
    }

    /// Collection recurses into non-test modules but skips test modules.
    #[test]
    fn impl_member_items_skips_test_modules_only() {
        let source = "//! doc\nmod inner {\n    pub struct S;\n    impl S { pub fn a(&self) {} }\n}\n\
            #[cfg(test)]\nmod tests {\n    pub struct T;\n    impl T { pub fn b(&self) {} }\n}\n";
        let parsed = parse_source(source).unwrap();

        let members = impl_member_items(source, parsed.syntax_tree());

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name().as_deref(), Some("a"));
    }

    /// Associated consts and types are members too; `use` inside an impl is
    /// not a documentable member.
    #[test]
    fn impl_member_items_covers_associated_consts_and_types() {
        let source = "//! doc\npub struct S;\nimpl S {\n    const C: u32 = 1;\n    type T = u32;\n    use std::io;\n}\n";
        let parsed = parse_source(source).unwrap();

        let members = impl_member_items(source, parsed.syntax_tree());

        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m.name().as_deref() == Some("C")));
        assert!(members.iter().any(|m| m.name().as_deref() == Some("T")));
    }

    /// A doc run survives an error-recovery node between it and the member:
    /// unrecognized nodes are transparent, as at top level.
    #[test]
    fn impl_member_items_keeps_docs_across_error_nodes() {
        let source = "//! doc\npub struct S;\nimpl S {\n    /// Docs.\n    @@@ junk tokens\n    pub fn a(&self) {}\n}\n";
        let parsed = parse_source(source).unwrap();

        let members = impl_member_items(source, parsed.syntax_tree());

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name().as_deref(), Some("a"));
        assert_eq!(members[0].doc_comments(), &[" Docs."]);
    }
}
