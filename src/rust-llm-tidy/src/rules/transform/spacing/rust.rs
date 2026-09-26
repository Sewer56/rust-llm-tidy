//! Insert missing blank lines between documented members of Rust
//! bodies.
//!
//! A member's attributes and outer doc comments attach to it. A gap
//! between two members is fixed when either member is documented,
//! the members sit on different lines, and no blank line separates
//! them.
//!
//! The blank line goes before the first attached node. Tuple structs
//! carry no braced members and are skipped.

use super::{SpacingEdit, attachment_line_start, emit, has_blank_line, overlaps_span};
use crate::languages::rust::parse::is_outer_doc;
use crate::source::{ItemKind, ParseResult};
use core::ops::Range;
use std::borrow::Cow;

/// The previous member of a body, kept for the gap check against the
/// next member.
struct PrevMember<'a> {
    /// The member node, kept for its label and span.
    node: tree_sitter::Node<'a>,

    /// Byte offset where the member ends.
    end: usize,

    /// True when the member carries attributes or docs of its own.
    documented: bool,
}

/// Insert missing blank lines between documented members.
///
/// A tree that recovered from syntax errors is never edited: the
/// function borrows `source` back and returns no edits.
pub(crate) fn fix_rust<'a>(
    source: &'a str,
    parsed: &ParseResult,
    protected: &[Range<usize>],
) -> (Cow<'a, str>, Vec<SpacingEdit>) {
    if parsed.syntax_tree().root_node().has_error() {
        return (Cow::Borrowed(source), Vec::new());
    }
    let mut edits = Vec::new();
    walk_items(
        parsed.syntax_tree().root_node(),
        source,
        protected,
        &mut edits,
    );
    emit(source, edits)
}

/// Depth-first cursor walk over `root`, fixing every member-list item.
fn walk_items<'a>(
    root: tree_sitter::Node<'a>,
    source: &str,
    protected: &[Range<usize>],
    out: &mut Vec<SpacingEdit>,
) {
    let mut cursor = root.walk();
    'walk: loop {
        check_item(cursor.node(), source, protected, out);
        if cursor.goto_first_child() {
            continue 'walk;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == root {
                return;
            }
        }
    }
}

/// Checks `node`'s member list when `node` declares one.
///
/// Attributes, comments, and doc comments preceding a member bind to
/// it as siblings, so the walk groups them into the member they
/// document.
///
/// A gap is edited when either member is documented, the members sit
/// on different lines, and no blank line separates them. The
/// attachment line must hold nothing but the attachment, and no
/// protected range may overlap the two members.
fn check_item<'a>(
    node: tree_sitter::Node<'a>,
    source: &str,
    protected: &[Range<usize>],
    out: &mut Vec<SpacingEdit>,
) {
    let Some(kind) = item_kind(node.kind()) else {
        return;
    };
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if body.kind() == "ordered_field_declaration_list" {
        return;
    }

    let mut prev: Option<PrevMember<'a>> = None;
    // First attached node of the next member: attribute, comment, or
    // the member itself.
    let mut attach: Option<tree_sitter::Node<'a>> = None;
    let mut has_attrs = false;
    let mut has_doc = false;
    for i in 0..body.named_child_count() as u32 {
        let Some(child) = body.named_child(i) else {
            continue;
        };
        match child.kind() {
            // Inner attributes and empty statements belong to the body
            // itself: they open no member gap.
            "inner_attribute_item" | "empty_statement" => {}
            "attribute_item" => {
                attach.get_or_insert(child);
                has_attrs = true;
            }
            "line_comment" | "block_comment" => {
                // A comment sharing the previous member's end row
                // trails that member; only later-row comments lead
                // the next one.
                let row = child.start_position().row;
                if prev
                    .as_ref()
                    .is_none_or(|p| p.node.end_position().row != row)
                {
                    attach.get_or_insert(child);
                }
                if !has_doc && is_outer_doc(child) {
                    has_doc = true;
                }
            }
            _ => {
                let attach_node = attach.take().unwrap_or(child);
                let documented = has_attrs || has_doc;
                has_attrs = false;
                has_doc = false;

                let prev_end = prev.as_ref().map_or(body.start_byte(), |p| p.end);
                let gap = &source[prev_end..attach_node.start_byte()];
                if let Some(p) = prev.as_ref()
                    && (documented || p.documented)
                    && gap.contains('\n')
                    && !has_blank_line(gap)
                    && let Some(byte) = attachment_line_start(attach_node, source)
                    && !protected
                        .iter()
                        .any(|r| overlaps_span(r, p.node.start_byte(), child.end_byte()))
                {
                    out.push(SpacingEdit {
                        byte,
                        line: attach_node.start_position().row as u32 + 1,
                        kind,
                        prev: member_label(p.node, source),
                        next: member_label(child, source),
                    });
                }
                prev = Some(PrevMember {
                    node: child,
                    end: child.end_byte(),
                    documented,
                });
            }
        }
    }
}

/// The enclosing item kind for one member-list item kind.
///
/// Items without a member list return `None`.
fn item_kind(kind: &str) -> Option<ItemKind> {
    match kind {
        "struct_item" => Some(ItemKind::Struct),
        "union_item" => Some(ItemKind::Union),
        "enum_item" => Some(ItemKind::Enum),
        "trait_item" => Some(ItemKind::Trait),
        "impl_item" => Some(ItemKind::Impl),
        _ => None,
    }
}

/// A member's declared name, or a phrase for its node kind.
fn member_label(member: tree_sitter::Node<'_>, source: &str) -> Box<str> {
    if let Some(text) = member
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
    {
        return Box::from(text);
    }
    member_kind_phrase(member.kind()).into()
}

/// Label phrase for a member node without a `name` field.
fn member_kind_phrase(kind: &str) -> &'static str {
    match kind {
        "function_item" | "function_signature_item" => "fn",
        "const_item" => "const",
        "static_item" => "static",
        "type_item" => "type",
        "use_declaration" => "use",
        "macro_invocation" => "macro invocation",
        _ => "item",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;
    use indoc::indoc;
    use rstest::rstest;

    /// Runs the fix over `source` with no protected ranges.
    fn fixed(source: &str) -> (Cow<'_, str>, Vec<SpacingEdit>) {
        fix_rust(source, &parse_source(source).unwrap(), &[])
    }

    /// Asserts the result is a borrowed no-op with no edits.
    fn assert_noop(result: &(Cow<'_, str>, Vec<SpacingEdit>)) {
        assert!(matches!(result.0, Cow::Borrowed(_)));
        assert_eq!(result.1, Vec::new());
    }

    // ── documented gaps without blank lines ──

    #[rstest]
    #[case::struct_fields(
        indoc! {r#"
            /// An edge in the flow graph.
            pub struct Edge {
                /// Address of the instruction taking this path.
                pub source: u32,
                /// Address of the first instruction at the destination.
                pub target: u32,
            }
        "#},
        indoc! {r#"
            /// An edge in the flow graph.
            pub struct Edge {
                /// Address of the instruction taking this path.
                pub source: u32,

                /// Address of the first instruction at the destination.
                pub target: u32,
            }
        "#},
        ItemKind::Struct,
        5
    )]
    #[case::union_fields(
        indoc! {r#"
            /// A value viewed as raw bits or a float.
            pub union Word {
                /// The raw bits.
                bits: u32,
                /// The float view.
                wide: f32,
            }
        "#},
        indoc! {r#"
            /// A value viewed as raw bits or a float.
            pub union Word {
                /// The raw bits.
                bits: u32,

                /// The float view.
                wide: f32,
            }
        "#},
        ItemKind::Union,
        5
    )]
    #[case::enum_variants(
        indoc! {r#"
            /// How control leaves an instruction.
            pub enum EdgeKind {
                /// Follow the target when the condition holds.
                BranchTaken,
                /// Continue when the condition does not hold.
                BranchNotTaken,
            }
        "#},
        indoc! {r#"
            /// How control leaves an instruction.
            pub enum EdgeKind {
                /// Follow the target when the condition holds.
                BranchTaken,

                /// Continue when the condition does not hold.
                BranchNotTaken,
            }
        "#},
        ItemKind::Enum,
        5
    )]
    #[case::trait_members(
        indoc! {"
            /// A node in the flow graph.
            pub trait Node {
                /// Address of this node's first instruction.
                fn start(&self) -> u32;
                /// Address of this node's last instruction.
                fn end(&self) -> u32;
            }
        "},
        indoc! {"
            /// A node in the flow graph.
            pub trait Node {
                /// Address of this node's first instruction.
                fn start(&self) -> u32;

                /// Address of this node's last instruction.
                fn end(&self) -> u32;
            }
        "},
        ItemKind::Trait,
        5
    )]
    #[case::impl_members(
        indoc! {"
            /// docs
            pub struct Bytes;

            impl Bytes {
                /// Reads the first byte.
                pub fn first(&self) -> u32 {
                    0
                }
                /// Reads the last byte.
                ///
                /// Wrapped to cover multi-line docs.
                pub fn last(&self) -> u32 {
                    0
                }
            }
        "},
        indoc! {"
            /// docs
            pub struct Bytes;

            impl Bytes {
                /// Reads the first byte.
                pub fn first(&self) -> u32 {
                    0
                }

                /// Reads the last byte.
                ///
                /// Wrapped to cover multi-line docs.
                pub fn last(&self) -> u32 {
                    0
                }
            }
        "},
        ItemKind::Impl,
        9
    )]
    fn fix_rust_should_insert_one_blank_line_for_every_item_kind(
        #[case] source: &str,
        #[case] expected: &str,
        #[case] kind: ItemKind,
        #[case] line: u32,
    ) {
        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].kind, kind);
        assert_eq!(edits[0].line, line);
    }

    #[rstest]
    #[case::first_documented(
        indoc! {"
            struct S {
                /// docs for a.
                a: u32,
                b: u32,
            }
        "},
        4
    )]
    #[case::second_documented(
        indoc! {"
            struct S {
                a: u32,
                /// docs for b.
                b: u32,
            }
        "},
        3
    )]
    #[case::attribute_on_first(
        indoc! {"
            struct S {
                #[serde(default)]
                a: u32,
                b: u32,
            }
        "},
        4
    )]
    #[case::attribute_on_second(
        indoc! {"
            struct S {
                a: u32,
                #[serde(default)]
                b: u32,
            }
        "},
        3
    )]
    fn fix_rust_should_insert_when_only_one_member_is_documented(
        #[case] source: &str,
        #[case] line: u32,
    ) {
        let (_, edits) = fixed(source);

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, line);
    }

    #[test]
    fn fix_rust_should_edit_every_documented_gap_in_one_body() {
        let source = indoc! {"
            struct S {
                /// docs for a.
                a: u32,
                /// docs for b.
                b: u32,
                /// docs for c.
                c: u32,
            }
        "};
        let expected = indoc! {"
            struct S {
                /// docs for a.
                a: u32,

                /// docs for b.
                b: u32,

                /// docs for c.
                c: u32,
            }
        "};

        let (text, edits) = fixed(source);

        let lines: Vec<u32> = edits.iter().map(|e| e.line).collect();
        assert_eq!(text, expected);
        assert_eq!(lines, [4, 6]);
    }

    // ── attachment details ──

    #[test]
    fn fix_rust_should_preserve_indentation_of_the_attachment_line() {
        let source = indoc! {"
            fn make() -> u32 {
                struct Inner {
                    /// docs for a.
                    a: u32,
                    /// docs for b.
                    b: u32,
                }
                0
            }
        "};
        let expected = indoc! {"
            fn make() -> u32 {
                struct Inner {
                    /// docs for a.
                    a: u32,

                    /// docs for b.
                    b: u32,
                }
                0
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].kind, ItemKind::Struct);
    }

    #[test]
    fn fix_rust_should_insert_before_a_plain_comment_leading_the_docs() {
        let source = indoc! {"
            struct S {
                /// docs for a.
                a: u32,
                // note.
                /// docs for b.
                b: u32,
            }
        "};
        let expected = indoc! {"
            struct S {
                /// docs for a.
                a: u32,

                // note.
                /// docs for b.
                b: u32,
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 4);
    }

    #[rstest]
    #[case::line_comment(
        indoc! {"
            struct S {
                /// docs for a.
                a: u32, // note
                /// docs for b.
                b: u32,
            }
        "},
        indoc! {"
            struct S {
                /// docs for a.
                a: u32, // note

                /// docs for b.
                b: u32,
            }
        "},
        4
    )]
    #[case::block_comment(
        indoc! {"
            struct S {
                /// docs for a.
                a: u32, /* note */
                /// docs for b.
                b: u32,
            }
        "},
        indoc! {"
            struct S {
                /// docs for a.
                a: u32, /* note */

                /// docs for b.
                b: u32,
            }
        "},
        4
    )]
    fn fix_rust_should_keep_a_trailing_comment_on_the_prev_member_row(
        #[case] source: &str,
        #[case] expected: &str,
        #[case] line: u32,
    ) {
        let (text, edits) = fixed(source);

        // The trailing comment stays on the previous member's line.
        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, line);
    }

    #[test]
    fn fix_rust_should_insert_before_the_first_doc_line_when_docs_wrap() {
        let source = indoc! {"
            struct S {
                a: u32,
                /// First line.
                /// Second line.
                b: u32,
            }
        "};
        let expected = indoc! {"
            struct S {
                a: u32,

                /// First line.
                /// Second line.
                b: u32,
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 3);
    }

    // ── no-ops ──

    #[rstest]
    #[case::blank_line_between_documented(
        "struct S {\n    /// d a.\n    a: u32,\n\n    /// d b.\n    b: u32,\n}\n"
    )]
    #[case::whitespace_only_blank_line(
        "struct S {\n    /// d a.\n    a: u32,\n    \n    /// d b.\n    b: u32,\n}\n"
    )]
    #[case::crlf_blank_line(
        "struct S {\r\n    /// d a.\r\n    a: u32,\r\n\r\n    /// d b.\r\n    b: u32,\r\n}\r\n"
    )]
    #[case::undocumented_members("struct Point {\n    x: u32,\n    y: u32,\n}\n")]
    #[case::members_share_a_line(indoc! {"
            struct P {
                /// docs for x.
                x: u32, y: u32,
            }
        "})]
    #[case::empty_body("struct E {\n}\n")]
    #[case::single_documented_member("struct S {\n    /// docs for a.\n    a: u32,\n}\n")]
    #[case::unit_struct("struct U;\n")]
    #[case::tuple_struct("struct T(u32, u32);\n")]
    #[case::stray_semicolon_before_a_single_member(indoc! {"
        impl Foo {
            ;
            /// d for a.
            fn a(&self) {}
        }
    "})]
    #[case::quad_slash_comment(indoc! {"
        struct S {
            //// note.
            a: u32,
            b: u32,
        }
    "})]
    #[case::triple_star_block_comment(indoc! {"
        struct S {
            /*** not a doc. */
            a: u32,
            b: u32,
        }
    "})]
    fn fix_rust_should_borrow_when_members_are_exempt(#[case] source: &str) {
        assert_noop(&fixed(source));
    }

    #[test]
    fn fix_rust_should_edit_real_gaps_when_the_body_leads_with_an_inner_attribute() {
        let source = indoc! {"
            impl Foo {
                #![allow(dead_code)]
                /// d for a.
                fn a(&self) {}
                /// d for b.
                fn b(&self) {}
            }
        "};
        let expected = indoc! {"
            impl Foo {
                #![allow(dead_code)]
                /// d for a.
                fn a(&self) {}

                /// d for b.
                fn b(&self) {}
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].kind, ItemKind::Impl);
    }

    // ── line endings ──

    #[test]
    fn fix_rust_should_insert_crlf_when_the_file_uses_crlf() {
        let source =
            "struct S {\r\n    /// d a.\r\n    a: u32,\r\n    /// d b.\r\n    b: u32,\r\n}\r\n";
        let expected = "struct S {\r\n    /// d a.\r\n    a: u32,\r\n\r\n    /// d b.\r\n    b: u32,\r\n}\
             \r\n";

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
    }

    #[test]
    fn fix_rust_should_follow_the_terminator_before_each_insertion() {
        // The first gap ends with CRLF, the second with LF; each
        // inserted blank line copies its neighbour.
        let source = "struct S {\n    /// d a.\n    a: u32,\r\n    /// d b.\r\n    b: u32,\n    \
                      /// d c.\n    c: u32,\n}\n";
        let expected = "struct S {\n    /// d a.\n    a: u32,\r\n\r\n    /// d b.\r\n    b: u32,\
                        \n\n    /// d c.\n    c: u32,\n}\n";

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 2);
    }

    // ── damaged trees ──

    #[test]
    fn fix_rust_should_borrow_when_the_tree_recovered_from_an_error() {
        let source = "struct S {\n    /// d a.\n    a: u32,\n    /// d b.\n    b: u32\n";

        assert_noop(&fixed(source));
    }

    // ── protected ranges ──

    /// A three-member body used by the protected-range tests.
    fn three_members() -> &'static str {
        indoc! {"
            struct S {
                /// d a.
                a: u32,
                /// d b.
                b: u32,
                /// d c.
                c: u32,
            }
        "}
    }

    #[rstest]
    #[case::prev_member(0, 1)]
    #[case::next_member(1, 1)]
    #[case::enclosing_body(2, 0)]
    fn fix_rust_should_skip_a_gap_overlapping_a_protected_range(
        #[case] protect: usize,
        #[case] expect_edits: usize,
    ) {
        let source = three_members();
        let anchors = [
            (source.find("a: u32").unwrap(), 1),
            (source.find("c: u32").unwrap(), 1),
            (source.find("struct S").unwrap(), source.len()),
        ];
        let (start, len) = anchors[protect];
        let protected = vec![start..start + len];
        let parsed = &parse_source(source).unwrap();

        let (_, edits) = fix_rust(source, parsed, &protected);

        assert_eq!(edits.len(), expect_edits);
    }

    #[test]
    fn fix_rust_should_edit_a_gap_when_a_range_only_touches_its_boundary() {
        let source = three_members();
        let a = source.find("a: u32").unwrap();
        let protected = vec![0..a];
        let parsed = &parse_source(source).unwrap();

        let (_, edits) = fix_rust(source, parsed, &protected);

        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn fix_rust_should_edit_an_unprotected_neighbor_gap() {
        let source = three_members();
        let a = source.find("a: u32").unwrap();
        // Overlaps the first pair (a and b) only; the c gap stays
        // editable.
        let protected = vec![a..a + 1];
        let parsed = &parse_source(source).unwrap();

        let (_, edits) = fix_rust(source, parsed, &protected);

        let lines: Vec<u32> = edits.iter().map(|e| e.line).collect();
        assert_eq!(lines, [6]);
    }

    // ── rewrite invariants ──

    #[test]
    fn fix_rust_should_be_idempotent() {
        let (text, _) = fixed(three_members());

        let reparsed = &parse_source(&text).unwrap();
        let (second, edits) = fix_rust(&text, reparsed, &[]);

        assert!(matches!(second, Cow::Borrowed(_)));
        assert_eq!(edits, Vec::new());
    }

    #[test]
    fn fix_rust_should_only_insert_line_terminators() {
        let source = three_members();

        let (text, edits) = fixed(source);

        // Each LF insertion adds exactly one byte; nothing else moves.
        assert_eq!(text.len(), source.len() + edits.len());
        let trimmed: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(trimmed, source.lines().collect::<Vec<_>>());
    }

    // ── golden case ──

    #[test]
    fn fix_rust_should_fix_the_local_edge_example() {
        let source = indoc! {r#"
            pub(in crate::analysis) struct LocalEdge {
                /// Address of the instruction taking this path.
                pub source: u32,
                /// Address of the first instruction at the destination.
                pub target: u32,
            }
        "#};
        let expected = indoc! {r#"
            pub(in crate::analysis) struct LocalEdge {
                /// Address of the instruction taking this path.
                pub source: u32,

                /// Address of the first instruction at the destination.
                pub target: u32,
            }
        "#};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].kind, ItemKind::Struct);
        assert_eq!(edits[0].line, 4);
        assert_eq!(&*edits[0].prev, "source");
        assert_eq!(&*edits[0].next, "target");
    }
}
