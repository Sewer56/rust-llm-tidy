//! Insert missing blank lines between documented members of C#
//! bodies.
//!
//! A member's doc comments (`///`, `/**`) and any preprocessor
//! conditional bind to the member that follows. A gap between two
//! members on different lines is fixed when either member is
//! documented and no blank line separates them.
//!
//! Other preprocessor directives annotate the body and open no gap.
//! Members directly inside a conditional stay unchecked: they sit in
//! the conditional's subtree, not the member list.

use super::{SpacingEdit, attachment_line_start, emit, has_blank_line, overlaps_span};
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

    /// True when the member carries docs or attributes of its own.
    documented: bool,
}

/// Insert missing blank lines between documented members.
///
/// A tree that recovered from syntax errors is never edited: the
/// function borrows `source` back and returns no edits.
pub(crate) fn fix_csharp<'a>(
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
/// Doc comments precede their member as `comment` siblings, so the
/// walk binds them, and any preprocessor conditional, to the member
/// that follows.
///
/// The blank line goes before the first attached node. A comment
/// ahead of a conditional keeps the boundary; otherwise the `#if`
/// line opens the inserted gap.
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
    if !matches!(
        body.kind(),
        "declaration_list" | "enum_member_declaration_list"
    ) {
        return;
    }

    let mut prev: Option<PrevMember<'a>> = None;
    // First attached node of the next member: comment, preprocessor
    // conditional, or the member itself.
    let mut attach: Option<tree_sitter::Node<'a>> = None;
    let mut has_doc = false;
    for i in 0..body.named_child_count() as u32 {
        let Some(child) = body.named_child(i) else {
            continue;
        };
        match child.kind() {
            "comment" => {
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
                if !has_doc && is_doc_comment(child, source) {
                    has_doc = true;
                }
            }
            "preproc_if" => {
                attach.get_or_insert(child);
            }
            // Other preprocessor directives annotate the body, not a
            // member: they open no member gap.
            directive if directive.starts_with("preproc_") => {}
            _ => {
                let attach_node = attach.take().unwrap_or(child);
                let documented = has_doc || named_child_of_kind(child, "attribute_list").is_some();
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

/// True when the comment node is a doc comment (`///` or `/**`).
fn is_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> bool {
    node.utf8_text(source.as_bytes())
        .is_ok_and(|text| text.starts_with("///") || text.starts_with("/**"))
}

/// The enclosing item kind for one member-list item kind.
///
/// Items without a member body return `None`.
fn item_kind(kind: &str) -> Option<ItemKind> {
    match kind {
        "class_declaration" => Some(ItemKind::Class),
        "struct_declaration" => Some(ItemKind::Struct),
        "interface_declaration" => Some(ItemKind::Interface),
        "record_declaration" => Some(ItemKind::Record),
        "enum_declaration" => Some(ItemKind::Enum),
        _ => None,
    }
}

/// A member's declared name, or a phrase for its node kind.
///
/// Fields and event fields carry no `name` field of their own, so
/// their label comes from the declared variable's name.
fn member_label(member: tree_sitter::Node<'_>, source: &str) -> Box<str> {
    if let Some(text) = member
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
    {
        return Box::from(text);
    }
    declared_field_name(member, source).unwrap_or_else(|| member_kind_phrase(member.kind()).into())
}

/// The variable name of a field or event field declaration, if any.
fn declared_field_name(member: tree_sitter::Node<'_>, source: &str) -> Option<Box<str>> {
    if !matches!(
        member.kind(),
        "field_declaration" | "event_field_declaration"
    ) {
        return None;
    }
    let declaration = named_child_of_kind(member, "variable_declaration")?;
    let declarator = named_child_of_kind(declaration, "variable_declarator")?;
    let name = declarator.child_by_field_name("name")?;
    let text = name.utf8_text(source.as_bytes()).ok()?;
    Some(text.into())
}

/// Label phrase for a member node without a usable name.
fn member_kind_phrase(kind: &str) -> &'static str {
    match kind {
        "method_declaration" => "method",
        "property_declaration" => "property",
        "constructor_declaration" => "constructor",
        "field_declaration" => "field",
        "event_field_declaration" => "event field",
        "class_declaration" => "class",
        "struct_declaration" => "struct",
        "interface_declaration" => "interface",
        "record_declaration" => "record",
        _ => "member",
    }
}

/// The first named child of `node` with `kind`, if any.
fn named_child_of_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    (0..node.named_child_count() as u32).find_map(|i| {
        let child = node.named_child(i).expect("index below count");
        (child.kind() == kind).then_some(child)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;
    use indoc::indoc;
    use rstest::rstest;

    /// Runs the fix over `source` with no protected ranges.
    fn fixed(source: &str) -> (Cow<'_, str>, Vec<SpacingEdit>) {
        fix_csharp(source, &parse(source).expect("fixture parses"), &[])
    }

    /// Asserts the result is a borrowed no-op with no edits.
    fn assert_noop(result: &(Cow<'_, str>, Vec<SpacingEdit>)) {
        assert!(matches!(result.0, Cow::Borrowed(_)));
        assert_eq!(result.1, Vec::new());
    }

    // ── documented gaps without blank lines ──

    #[rstest]
    #[case::class(
        indoc! {"
            /// An edge in a flow graph.
            public class Edge
            {
                /// <summary>The source address.</summary>
                public int Source;
                /// <summary>The target address.</summary>
                public int Target;
            }
        "},
        indoc! {"
            /// An edge in a flow graph.
            public class Edge
            {
                /// <summary>The source address.</summary>
                public int Source;

                /// <summary>The target address.</summary>
                public int Target;
            }
        "},
        ItemKind::Class
    )]
    #[case::structure(
        indoc! {"
            /// A value pair.
            public struct Pair
            {
                /// <summary>The low half.</summary>
                public int Low;
                /// <summary>The high half.</summary>
                public int High;
            }
        "},
        indoc! {"
            /// A value pair.
            public struct Pair
            {
                /// <summary>The low half.</summary>
                public int Low;

                /// <summary>The high half.</summary>
                public int High;
            }
        "},
        ItemKind::Struct
    )]
    #[case::interface(
        indoc! {"
            /// Reads bytes.
            public interface IReader
            {
                /// <summary>Reads the first byte.</summary>
                int First();
                /// <summary>Reads the last byte.</summary>
                int Last();
            }
        "},
        indoc! {"
            /// Reads bytes.
            public interface IReader
            {
                /// <summary>Reads the first byte.</summary>
                int First();

                /// <summary>Reads the last byte.</summary>
                int Last();
            }
        "},
        ItemKind::Interface
    )]
    #[case::record(
        indoc! {"
            /// A located value.
            public record Spot
            {
                /// <summary>The row.</summary>
                public int Row { get; init; }
                /// <summary>The column.</summary>
                public int Column { get; init; }
            }
        "},
        indoc! {"
            /// A located value.
            public record Spot
            {
                /// <summary>The row.</summary>
                public int Row { get; init; }

                /// <summary>The column.</summary>
                public int Column { get; init; }
            }
        "},
        ItemKind::Record
    )]
    #[case::enumeration(
        indoc! {"
            /// How control leaves an instruction.
            public enum Exit
            {
                /// <summary>Follows the branch.</summary>
                Taken,
                /// <summary>Falls through.</summary>
                NotTaken,
            }
        "},
        indoc! {"
            /// How control leaves an instruction.
            public enum Exit
            {
                /// <summary>Follows the branch.</summary>
                Taken,

                /// <summary>Falls through.</summary>
                NotTaken,
            }
        "},
        ItemKind::Enum
    )]
    fn fix_csharp_should_insert_one_blank_line_for_every_item_kind(
        #[case] source: &str,
        #[case] expected: &str,
        #[case] kind: ItemKind,
    ) {
        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].kind, kind);
        assert_eq!(edits[0].line, 6);
    }

    #[rstest]
    #[case::first_documented(
        indoc! {"
            class C {
                /// <summary>d for a.</summary>
                int a;
                int b;
            }
        "},
        4
    )]
    #[case::second_documented(
        indoc! {"
            class C {
                int a;
                /// <summary>d for b.</summary>
                int b;
            }
        "},
        3
    )]
    #[case::attribute_on_first(
        indoc! {"
            class C {
                [SerializeField]
                int a;
                int b;
            }
        "},
        4
    )]
    #[case::attribute_on_second(
        indoc! {"
            class C {
                int a;
                [SerializeField]
                int b;
            }
        "},
        3
    )]
    fn fix_csharp_should_insert_when_only_one_member_is_documented(
        #[case] source: &str,
        #[case] line: u32,
    ) {
        let (_, edits) = fixed(source);

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, line);
    }

    #[test]
    fn fix_csharp_should_edit_members_of_nested_types() {
        let source = indoc! {"
            class Outer {
                /// d for inner.
                class Inner {
                    /// d for x.
                    int x;
                    /// d for y.
                    int y;
                }
            }
        "};
        let expected = indoc! {"
            class Outer {
                /// d for inner.
                class Inner {
                    /// d for x.
                    int x;

                    /// d for y.
                    int y;
                }
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].kind, ItemKind::Class);
    }

    #[test]
    fn fix_csharp_should_edit_every_documented_gap_in_one_body() {
        let source = indoc! {"
            class S {
                /// d for a.
                int a;
                /// d for b.
                int b;
                /// d for c.
                int c;
            }
        "};
        let expected = indoc! {"
            class S {
                /// d for a.
                int a;

                /// d for b.
                int b;

                /// d for c.
                int c;
            }
        "};

        let (text, edits) = fixed(source);

        let lines: Vec<u32> = edits.iter().map(|e| e.line).collect();
        assert_eq!(text, expected);
        assert_eq!(lines, [4, 6]);
    }

    // ── attachment details ──

    #[test]
    fn fix_csharp_should_insert_before_a_plain_comment_leading_the_docs() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a;
                // note.
                /// d for b.
                int b;
            }
        "};
        let expected = indoc! {"
            class C {
                /// d for a.
                int a;

                // note.
                /// d for b.
                int b;
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 4);
    }

    #[test]
    fn fix_csharp_should_keep_a_trailing_comment_on_the_prev_member_row() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a; // note
                /// d for b.
                int b;
            }
        "};
        let expected = indoc! {"
            class C {
                /// d for a.
                int a; // note

                /// d for b.
                int b;
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 4);
    }

    #[test]
    fn fix_csharp_should_insert_before_the_first_doc_line_when_docs_wrap() {
        let source = indoc! {"
            class C {
                int a;
                /// First line.
                /// Second line.
                int b;
            }
        "};
        let expected = indoc! {"
            class C {
                int a;

                /// First line.
                /// Second line.
                int b;
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 3);
    }

    #[test]
    fn fix_csharp_should_label_fields_by_their_declared_variable() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a;
                /// d for b.
                int b;
            }
        "};

        let (_, edits) = fixed(source);

        assert_eq!(&*edits[0].prev, "a");
        assert_eq!(&*edits[0].next, "b");
    }

    // ── no-ops ──

    #[rstest]
    #[case::blank_line_between_documented(
        indoc! {"
            class S {
                /// d for a.
                int a;

                /// d for b.
                int b;
            }
        "}
    )]
    #[case::whitespace_only_blank_line(
        "class S {\n    /// d a.\n    int a;\n    \n    /// d b.\n    int b;\n}\n"
    )]
    #[case::crlf_blank_line(
        "class S {\r\n    /// d a.\r\n    int a;\r\n\r\n    /// d b.\r\n    int b;\r\n}\r\n"
    )]
    #[case::undocumented_members(indoc! {"
            class P {
                int x;
                int y;
            }
        "})]
    #[case::members_share_a_line(indoc! {"
            class P {
                /// <summary>d for x.</summary>
                int x; int y;
            }
        "})]
    #[case::empty_body(indoc! {"
            class E {
            }
        "})]
    #[case::single_documented_member(indoc! {"
            class S {
                /// d for a.
                int a;
            }
        "})]
    #[case::positional_record("public record Spot(int Row, int Column);\n")]
    fn fix_csharp_should_borrow_when_members_are_exempt(#[case] source: &str) {
        assert_noop(&fixed(source));
    }

    // ── preprocessor directives ──

    #[test]
    fn fix_csharp_should_insert_before_the_conditional_when_it_leads_the_gap() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a;
            #if DEBUG
                int b;
            #endif
                /// d for c.
                int c;
            }
        "};
        let expected = indoc! {"
            class C {
                /// d for a.
                int a;

            #if DEBUG
                int b;
            #endif
                /// d for c.
                int c;
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 4);
        assert_eq!(&*edits[0].prev, "a");
        assert_eq!(&*edits[0].next, "c");
    }

    #[test]
    fn fix_csharp_should_insert_before_a_comment_that_leads_the_conditional() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a;
                // note.
            #if DEBUG
                int b;
            #endif
                /// d for c.
                int c;
            }
        "};
        let expected = indoc! {"
            class C {
                /// d for a.
                int a;

                // note.
            #if DEBUG
                int b;
            #endif
                /// d for c.
                int c;
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 4);
    }

    #[test]
    fn fix_csharp_should_leave_members_inside_a_conditional_unchecked() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a;
            #if DEBUG
                /// d for b.
                int b;
            #endif
            }
        "};

        assert_noop(&fixed(source));
    }

    #[test]
    fn fix_csharp_should_still_edit_across_a_region_directive() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a;
            #region Edges
                /// d for b.
                int b;
            #endregion
            }
        "};
        let expected = indoc! {"
            class C {
                /// d for a.
                int a;
            #region Edges

                /// d for b.
                int b;
            #endregion
            }
        "};

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].line, 5);
    }

    // ── line endings ──

    #[test]
    fn fix_csharp_should_insert_crlf_when_the_file_uses_crlf() {
        let source =
            "class S {\r\n    /// d a.\r\n    int a;\r\n    /// d b.\r\n    int b;\r\n}\r\n";
        let expected =
            "class S {\r\n    /// d a.\r\n    int a;\r\n\r\n    /// d b.\r\n    int b;\r\n}\r\n";

        let (text, edits) = fixed(source);

        assert_eq!(text, expected);
        assert_eq!(edits.len(), 1);
    }

    // ── damaged trees ──

    #[test]
    fn fix_csharp_should_borrow_when_the_tree_recovered_from_an_error() {
        let source = "class C {\n    /// d a.\n    int a;\n    /// d b.\n    int b;\n";

        assert_noop(&fixed(source));
    }

    // ── protected ranges ──

    /// A two-gap body used by the protected-range tests.
    fn three_members() -> &'static str {
        indoc! {"
            class S {
                /// d a.
                int a;
                /// d b.
                int b;
                /// d c.
                int c;
            }
        "}
    }

    #[rstest]
    #[case::prev_member(0, 1)]
    #[case::next_member(1, 1)]
    #[case::enclosing_body(2, 0)]
    fn fix_csharp_should_skip_a_gap_overlapping_a_protected_range(
        #[case] protect: usize,
        #[case] expect_edits: usize,
    ) {
        let source = three_members();
        let anchors = [
            (source.find("int a;").unwrap(), 1),
            (source.find("int c;").unwrap(), 1),
            (source.find("class S").unwrap(), source.len()),
        ];
        let (start, len) = anchors[protect];
        let protected = vec![start..start + len];
        let parsed = &parse(source).expect("fixture parses");

        let (_, edits) = fix_csharp(source, parsed, &protected);

        assert_eq!(edits.len(), expect_edits);
    }

    #[test]
    fn fix_csharp_should_edit_a_gap_when_a_range_only_touches_its_boundary() {
        let source = three_members();
        let a = source.find("int a;").unwrap();
        let protected = vec![0..a];
        let parsed = &parse(source).expect("fixture parses");

        let (_, edits) = fix_csharp(source, parsed, &protected);

        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn fix_csharp_should_edit_an_unprotected_neighbor_gap() {
        let source = three_members();
        let a = source.find("int a;").unwrap();
        // Overlaps the first pair (a and b) only; the c gap stays
        // editable.
        let protected = vec![a..a + 1];
        let parsed = &parse(source).expect("fixture parses");

        let (_, edits) = fix_csharp(source, parsed, &protected);

        let lines: Vec<u32> = edits.iter().map(|e| e.line).collect();
        assert_eq!(lines, [6]);
    }

    // ── rewrite invariants ──

    #[test]
    fn fix_csharp_should_be_idempotent() {
        let (text, _) = fixed(three_members());

        let reparsed = &parse(&text).expect("output parses");
        let (second, edits) = fix_csharp(&text, reparsed, &[]);

        assert!(matches!(second, Cow::Borrowed(_)));
        assert_eq!(edits, Vec::new());
    }

    #[test]
    fn fix_csharp_should_only_insert_line_terminators() {
        let source = three_members();

        let (text, edits) = fixed(source);

        // Each LF insertion adds exactly one byte; nothing else moves.
        assert_eq!(text.len(), source.len() + edits.len());
        let trimmed: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(trimmed, source.lines().collect::<Vec<_>>());
    }
}
