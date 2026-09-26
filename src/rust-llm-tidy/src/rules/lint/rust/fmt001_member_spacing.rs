//! Keep documented members apart with blank lines.
//!
//! `FMT001` ([`check`]) fires when members of a braced body sit on
//! different lines with no blank line between them. Either member may
//! carry the docs or attributes that flag the gap.
//!
//! The five body kinds are `struct`, `union`, `enum`, `trait`, and
//! `impl`.
//!
//! A member's doc comments and attributes belong to it, so the blank
//! line goes before them. Undocumented members may stay packed.
//! Members sharing one line are exempt: there is no room for a blank
//! line.

use crate::languages::rust::parse::is_outer_doc;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_FMT001;
use crate::source::ParseResult;

/// One member gap without a separating blank line.
struct MemberGap<'a> {
    /// The member before the gap, named first in the message.
    prev: tree_sitter::Node<'a>,

    /// The member after the gap; the blank line belongs before it.
    next: tree_sitter::Node<'a>,

    /// 0-based row of the first doc comment attached to the next
    /// member, if any.
    doc_row: Option<usize>,

    /// 0-based row of the next member's first attached node.
    next_start_row: usize,
}

/// The previous member of a body, kept for the gap check against the
/// next member.
struct PrevMember<'a> {
    /// The member node, kept for its diagnostic label.
    node: tree_sitter::Node<'a>,

    /// Byte offset where the member ends.
    end: usize,

    /// True when the member carries attributes or docs of its own.
    documented: bool,
}

/// Flag missing blank lines between documented members.
///
/// Walks the retained tree-sitter tree once. Every braced member list
/// yields one warning per gap that lacks a blank line while either
/// member carries docs or attributes.
///
/// The walk skips tuple structs: they carry no braced members. Nothing
/// re-parses.
///
/// # Arguments
///
/// - `parsed` - the parse whose tree the walk visits.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    let source = parsed.source.as_str();
    let mut diags = Vec::new();
    walk_items(parsed.syntax_tree().root_node(), source, &mut diags);
    diags
}

/// Depth-first cursor walk over `root`, checking every member-list item.
fn walk_items<'a>(root: tree_sitter::Node<'a>, source: &'a str, out: &mut Vec<Diagnostic>) {
    let mut cursor = root.walk();
    'walk: loop {
        check_item(cursor.node(), source, out);
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
/// The walk skips tuple structs: an ordered field list has no braced
/// members. `mod` and `extern` bodies share the `declaration_list` kind
/// with traits and impls but never match an item kind here.
fn check_item<'a>(node: tree_sitter::Node<'a>, source: &str, out: &mut Vec<Diagnostic>) {
    let Some((kind, noun)) = item_shape(node.kind()) else {
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
    let mut attach: Option<(usize, usize)> = None;
    let mut has_attrs = false;
    let mut doc_row: Option<usize> = None;
    for i in 0..body.named_child_count() as u32 {
        let Some(child) = body.named_child(i) else {
            continue;
        };
        match child.kind() {
            // Inner attributes and empty statements belong to the body
            // itself: they open no member gap.
            "inner_attribute_item" | "empty_statement" => {}
            "attribute_item" => {
                attach.get_or_insert((child.start_byte(), child.start_position().row));
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
                    attach.get_or_insert((child.start_byte(), row));
                }
                if doc_row.is_none() && is_outer_doc(child) {
                    doc_row = Some(row);
                }
            }
            _ => {
                let (start, start_row) = attach
                    .take()
                    .unwrap_or((child.start_byte(), child.start_position().row));
                let documented = has_attrs || doc_row.is_some();
                has_attrs = false;
                let gap_doc_row = doc_row.take();

                let prev_end = prev.as_ref().map_or(body.start_byte(), |p| p.end);
                let gap = &source[prev_end..start];
                if let Some(p) = prev.as_ref()
                    && (documented || p.documented)
                    && gap.contains('\n')
                    && !has_blank_line(gap)
                {
                    out.push(gap_diagnostic(
                        kind,
                        noun,
                        &declared_name(node, source),
                        source,
                        MemberGap {
                            prev: p.node,
                            next: child,
                            doc_row: gap_doc_row,
                            next_start_row: start_row,
                        },
                    ));
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

/// The item's declared name, or the implemented type for `impl` blocks.
fn declared_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let field = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("type"))?;
    field.utf8_text(source.as_bytes()).ok().map(str::to_string)
}

/// Builds the warning for one unseparated member gap.
///
/// The diagnostic line anchors at the next member's first doc comment,
/// or at its first attached node when it carries none. The finding then
/// sits where the blank line belongs.
fn gap_diagnostic(
    kind: &'static str,
    noun: &'static str,
    item_name: &Option<String>,
    source: &str,
    gap: MemberGap<'_>,
) -> Diagnostic {
    let a = member_label(gap.prev, source);
    let b = member_label(gap.next, source);
    let item = item_name.as_deref().unwrap_or(kind);
    Diagnostic {
        title: Some("missing blank line between documented members".into()),
        severity: Severity::Warning,
        code: CODE_FMT001,
        message: indoc::formatdoc! {"
            `{item}` {kind} {noun} `{a}` and `{b}` need a blank line between them.
            Why: a blank line keeps each doc comment attached to its own member.
            Suggestions:
            - Add one blank line between `{a}` and `{b}`, before any docs and
              attributes of `{b}`."},
        line: gap.doc_row.unwrap_or(gap.next_start_row) + 1,
        item_kind: kind.to_string(),
        item_name: item_name.clone(),
    }
}

/// True when `gap` contains a complete blank line.
///
/// A blank line is a `\n` followed only by spaces, tabs, or `\r` and
/// another `\n`. Trailing indentation before the next member does not
/// count: it is not a complete line.
fn has_blank_line(gap: &str) -> bool {
    let bytes = gap.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\r') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\n' {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

/// The diagnostic kind and member noun for one member-list item kind.
///
/// Items without a member list return `None`.
fn item_shape(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "struct_item" => Some(("struct", "fields")),
        "union_item" => Some(("union", "fields")),
        "enum_item" => Some(("enum", "variants")),
        "trait_item" => Some(("trait", "members")),
        "impl_item" => Some(("impl", "members")),
        _ => None,
    }
}

/// A member's declared name, or a phrase for its node kind.
fn member_label(member: tree_sitter::Node<'_>, source: &str) -> String {
    member
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
        .map(str::to_string)
        .unwrap_or_else(|| member_kind_phrase(member.kind()).to_string())
}

/// Diagnostic phrase for a member node without a `name` field.
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
    use rstest::rstest;

    /// Parses `source` and runs FMT001 over it.
    fn checks(source: &str) -> Vec<Diagnostic> {
        check(&parse_source(source).unwrap())
    }

    // ── documented gaps without blank lines ──

    #[rstest]
    #[case::struct_fields(
        r#"/// An edge in the flow graph.
pub struct Edge {
    /// Address of the instruction taking this path.
    pub source: u32,
    /// Address of the first instruction at the destination.
    pub target: u32,
}
"#,
        "struct",
        "Edge",
        "fields",
        5
    )]
    #[case::union_fields(
        r#"/// A value viewed as raw bits or a float.
pub union Word {
    /// The raw bits.
    bits: u32,
    /// The float view.
    wide: f32,
}
"#,
        "union",
        "Word",
        "fields",
        5
    )]
    #[case::enum_variants(
        r#"/// How control leaves an instruction.
pub enum EdgeKind {
    /// Follow the target when the condition holds.
    BranchTaken,
    /// Continue when the condition does not hold.
    BranchNotTaken,
}
"#,
        "enum",
        "EdgeKind",
        "variants",
        5
    )]
    #[case::trait_members(
        r#"/// A node in the flow graph.
pub trait Node {
    /// Address of this node's first instruction.
    fn start(&self) -> u32;
    /// Address of this node's last instruction.
    fn end(&self) -> u32;
}
"#,
        "trait",
        "Node",
        "members",
        5
    )]
    #[case::impl_members(
        r#"/// docs
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
"#,
        "impl",
        "Bytes",
        "members",
        9
    )]
    fn check_should_flag_documented_gap_for_every_item_kind(
        #[case] source: &str,
        #[case] kind: &str,
        #[case] name: &str,
        #[case] noun: &str,
        #[case] line: usize,
    ) {
        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_FMT001);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[0].item_kind, kind);
        assert_eq!(diags[0].item_name.as_deref(), Some(name));
        assert!(diags[0].message.contains(noun));
        assert_eq!(diags[0].line, line);
    }

    #[rstest]
    #[case::first_documented("struct S {\n    /// docs for a.\n    a: u32,\n    b: u32,\n}\n", 4)]
    #[case::second_documented("struct S {\n    a: u32,\n    /// docs for b.\n    b: u32,\n}\n", 3)]
    #[case::attribute_on_first(
        "struct S {\n    #[serde(default)]\n    a: u32,\n    b: u32,\n}\n",
        4
    )]
    #[case::attribute_on_second(
        "struct S {\n    a: u32,\n    #[serde(default)]\n    b: u32,\n}\n",
        3
    )]
    fn check_should_flag_when_only_one_member_is_documented(
        #[case] source: &str,
        #[case] line: usize,
    ) {
        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, line);
    }

    #[test]
    fn check_should_explain_the_fix_when_struct_fields_are_documented() {
        let source = r#"struct S {
    /// docs for a.
    a: u32,
    /// docs for b.
    b: u32,
}
"#;

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
        assert_eq!(
            diags[0].message,
            "`S` struct fields `a` and `b` need a blank line between them.\n\
             Why: a blank line keeps each doc comment attached to its own member.\n\
             Suggestions:\n\
             - Add one blank line between `a` and `b`, before any docs and\n  \
             attributes of `b`."
        );
    }

    #[test]
    fn check_should_flag_the_flow_graph_edge_example() {
        let source = r#"pub(in crate::analysis) struct LocalEdge {
    /// Address of the instruction taking this path.
    pub source: u32,
    /// Address of the first instruction at the destination.
    pub target: u32,
}
"#;

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].item_name.as_deref(), Some("LocalEdge"));
        assert_eq!(diags[0].line, 4);
    }

    // ── exemptions: no room, no docs, no gap ──

    #[rstest]
    #[case::blank_line_between_documented(
        r#"struct S {
    /// docs for a.
    a: u32,

    /// docs for b.
    b: u32,
}
"#
    )]
    #[case::undocumented_members(
        r#"struct Point {
    x: u32,
    y: u32,
}
"#
    )]
    #[case::documented_members_share_a_line(
        "struct P {\n    /// docs for x.\n    x: u32, y: u32,\n}\n"
    )]
    #[case::quad_slash_comment("struct S {\n    //// note.\n    a: u32,\n    b: u32,\n}\n")]
    #[case::triple_star_block_comment(
        "struct S {\n    /*** not a doc. */\n    a: u32,\n    b: u32,\n}\n"
    )]
    #[case::stray_semicolon("impl Foo {\n    ;\n    /// docs for a.\n    fn a(&self) {}\n}\n")]
    #[case::unit_struct("struct U;\n")]
    #[case::tuple_struct("struct T(u32, u32);\n")]
    #[case::empty_body("struct E {\n}\n")]
    #[case::single_documented_member(
        r#"struct S {
    /// docs for a.
    a: u32,
}
"#
    )]
    fn check_should_stay_quiet_when_members_are_exempt(#[case] source: &str) {
        assert!(checks(source).is_empty());
    }

    // ── gap details: anchors, comments, line endings ──

    #[test]
    fn check_should_anchor_at_first_doc_line_when_docs_wrap() {
        let source =
            "struct S {\n    a: u32,\n    /// First line.\n    /// Second line.\n    b: u32,\n}\n";

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn check_should_flag_when_a_plain_comment_sits_between_members() {
        let source = "struct S {\n    /// docs for a.\n    a: u32,\n    // note.\n    /// docs for b.\n    b: u32,\n}\n";

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }

    #[rstest]
    #[case::line_comment(
        "struct S {\n    /// docs for a.\n    a: u32, // note\n    /// docs for b.\n    b: u32,\n}\n",
        4
    )]
    #[case::block_comment(
        "struct S {\n    /// docs for a.\n    a: u32, /* note */\n    /// docs for b.\n    b: u32,\n}\n",
        4
    )]
    fn check_should_flag_when_a_trailing_comment_shares_the_prev_member_row(
        #[case] source: &str,
        #[case] line: usize,
    ) {
        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, line);
    }

    #[test]
    fn check_should_anchor_at_the_member_when_only_a_trailing_comment_precedes_it() {
        let source = "struct S {\n    /// docs for a.\n    a: u32, // note\n    b: u32,\n}\n";

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }

    #[test]
    fn check_should_flag_every_documented_gap_in_one_body() {
        let source = r#"struct S {
    /// docs for a.
    a: u32,
    /// docs for b.
    b: u32,
    /// docs for c.
    c: u32,
}
"#;

        let lines: Vec<usize> = checks(source).iter().map(|d| d.line).collect();

        assert_eq!(lines, [4, 6]);
    }

    #[test]
    fn check_should_flag_members_of_local_structs_inside_fns() {
        let source = r#"fn make() -> u32 {
    struct Inner {
        /// docs for a.
        a: u32,
        /// docs for b.
        b: u32,
    }
    0
}
"#;

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].item_name.as_deref(), Some("Inner"));
        assert_eq!(diags[0].line, 5);
    }

    #[rstest]
    #[case::whitespace_only_line(
        "struct S {\n    /// docs for a.\n    a: u32,\n    \n    /// docs for b.\n    b: u32,\n}\n"
    )]
    #[case::crlf_blank_line(
        "struct S {\r\n    /// docs for a.\r\n    a: u32,\r\n\r\n    /// docs for b.\r\n    b: u32,\r\n}\r\n"
    )]
    fn check_should_stay_quiet_when_the_blank_line_has_whitespace(#[case] source: &str) {
        assert!(checks(source).is_empty());
    }

    #[test]
    fn check_should_flag_real_gaps_when_impl_body_leads_with_an_inner_attribute() {
        let source = "impl Foo {\n    #![allow(dead_code)]\n    /// d for a.\n    fn a(&self) {}\n    /// d for b.\n    fn b(&self) {}\n}\n";

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }

    #[test]
    fn check_should_flag_when_line_endings_are_crlf() {
        let source = "struct S {\r\n    /// docs for a.\r\n    a: u32,\r\n    /// docs for b.\r\n    b: u32,\r\n}\r\n";

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }
}
