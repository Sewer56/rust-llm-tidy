//! Provides span extraction, comment pinning, preamble/trailer detection.
//!
//! This module orchestrates parsing: it splits source text into top-level
//! items with byte spans (comment-pinning prefix comments/attributes to each
//! item), classifies them, and exposes the data model.
//!
//! Public types are re-exported from this module for downstream use.
//!
//! Parsing is performed with tree-sitter (the `tree-sitter-rust` grammar),
//! which yields byte offsets directly - no line/column conversion is needed.

use crate::parse::classify::{PendingTrivia, classify_item, is_attachable, is_transparent_comment};
pub use item::{ParseResult, SourceItem, VisibilityTier};
pub use kind::ItemKind;
pub use member::TypeMember;

mod classify;
mod item;
mod kind;
mod member;

/// A raw top-level item entry: the item body node (or the wrapping
/// `expression_statement` for a top-level macro invocation) plus its pending
/// attachable trivia (attributes + outer doc comments).
struct RawEntry<'a> {
    /// Node whose byte range covers the item body (incl. trailing `;` for
    /// macro invocations wrapped in `expression_statement`).
    body: tree_sitter::Node<'a>,
    /// Attachable trivia immediately preceding the item.
    pending: PendingTrivia<'a>,
}

/// Parse a Rust source file and extract items with spans.
///
/// Uses tree-sitter to parse the file and walk the syntax tree, extracting byte
/// spans for each top-level item. Comments and attributes that syntactically
/// precede an item are pinned to it (comment-pinning).
///
/// Spans are laid back-to-back so each item carries the blank lines and `//`
/// comments preceding it when reordered: each item's `end` is the byte after
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
/// - `rust_language` fails to construct the tree-sitter-rust language.
/// - `Parser::set_language` rejects the language (should not happen with the
///   bundled grammar).
/// - `Parser::parse` returns `None` (tree-sitter failed to produce a tree).
///
/// tree-sitter performs error recovery, so syntactically invalid Rust still
/// yields a tree (possibly with `ERROR` nodes) rather than a parse error.
pub fn parse_source(source: &str) -> anyhow::Result<ParseResult> {
    let lang = rust_language()?;
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

    Ok(ParseResult {
        items,
        source: source.to_string(),
        tree,
        preamble_end,
        trailer_start,
    })
}

/// The tree-sitter-rust grammar this crate parses with.
///
/// Exposed so language backends (the `rust-llm-tidy-lang` crate) reuse the
/// same grammar [`parse_source`] parses with instead of constructing their
/// own.
///
/// # Errors
///
/// Returns an error when the bundled tree-sitter-rust grammar cannot convert
/// into a [`tree_sitter::Language`] (cannot happen with the pinned grammar
/// version).
pub fn rust_language() -> anyhow::Result<tree_sitter::Language> {
    Ok(tree_sitter_rust::LANGUAGE.into())
}

/// Assign each item a span that carries the blank lines and `//` comments
/// preceding it, so reordering preserves that whitespace.
///
/// Spans are laid back-to-back: each item's `start` is the previous item's
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
/// the real leading docs/attrs; with no attached trivia it falls back to the
/// item body start.
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
        // (= preamble_end); later items chain from the previous item's end so
        // the inter-item gap falls inside this item's span.
        let start = if first {
            first = false;
            attached_start
        } else {
            prev_end
        };

        // `end` extends body_end to the byte after its terminating newline:
        // the first line-start strictly greater than body_end.
        let end = next_line_start(line_starts, body_end).unwrap_or(source_len);

        let start_line = line_of(line_starts, attached_start);

        let class = classify_item(body, source, &entry.pending);
        out.push(SourceItem::new(
            start,
            end,
            start_line,
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
        ));
        prev_end = end;
    }
    out
}

/// Walk the `source_file` children in byte order and collect one [`RawEntry`]
/// per recognized top-level item, attaching the contiguous run of preceding
/// attributes and outer doc comments to each.
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
            // Transparent: ignored, pending run preserved.
        } else if let Some(entry) = item_entry_for(child) {
            entries.push(RawEntry {
                body: entry,
                pending: std::mem::take(&mut pending),
            });
        } else {
            // Unrecognized non-item top-level node (e.g. a stray
            // `expression_statement` that is not a macro invocation): treat as
            // transparent so it does not break attachment of surrounding trivia.
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
    // Heuristic preallocation. Capacity = bytes/D; no regrowth when the file's
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

/// If `node` is a recognized top-level item, return the body node to classify.
///
/// Top-level macro invocations are wrapped in `expression_statement`; the
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

/// 1-based line number of `byte` (count of line-starts at or before `byte`).
fn line_of(line_starts: &[usize], byte: usize) -> usize {
    line_starts.partition_point(|&s| s <= byte)
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

#[cfg(test)]
mod tests {
    use super::parse_source;
    use crate::parse::ItemKind;

    /// Gap-anchored spans: each non-first item's `start` is the previous
    /// item's `end`, `end` includes the trailing newline, and `start_line`
    /// tracks the attached-trivia start (the SYN body start when no attached
    /// attrs/docs precede it).
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
}
