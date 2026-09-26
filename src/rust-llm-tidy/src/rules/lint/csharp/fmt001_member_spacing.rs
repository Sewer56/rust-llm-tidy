//! Keep documented members apart with blank lines.
//!
//! `FMT001` ([`check`]) fires when members of a type body sit on
//! different lines with no blank line between them. Either member may
//! carry the docs or attributes that flag the gap.
//!
//! The five body kinds are `class`, `struct`, `interface`, `record`,
//! and `enum`.
//!
//! A member's doc comments and attributes belong to it, so the blank
//! line goes before them. Undocumented members may stay packed.
//! Members sharing one line are exempt: there is no room for a blank
//! line.

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

    /// True when a preprocessor conditional sits in the gap.
    conditional: bool,

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
/// Walks the retained tree-sitter tree once. Every member list yields
/// one warning per gap that lacks a blank line while either member
/// carries docs or attributes. Nothing re-parses.
///
/// Members directly inside preprocessor conditionals stay unchecked:
/// they sit in the conditional's subtree, not the member list. Nested
/// type bodies inside one are still checked.
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
/// Doc comments precede their member as `comment` siblings, so the
/// walk binds them, and any preprocessor conditional, to the member
/// that follows.
///
/// Attributes sit inside the member node in this grammar. A member with
/// an `attribute_list` child counts as documented, and its own start
/// already covers the attributes.
fn check_item<'a>(node: tree_sitter::Node<'a>, source: &str, out: &mut Vec<Diagnostic>) {
    let Some(kind) = item_shape(node.kind()) else {
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
    let mut attach: Option<(usize, usize)> = None;
    let mut doc_row: Option<usize> = None;
    let mut conditional = false;
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
                    attach.get_or_insert((child.start_byte(), row));
                }
                if doc_row.is_none() && is_doc_comment(child, source) {
                    doc_row = Some(row);
                }
            }
            "preproc_if" => {
                if attach.is_none() {
                    conditional = true;
                }
                attach.get_or_insert((child.start_byte(), child.start_position().row));
            }
            // Other preprocessor directives annotate the body, not a
            // member: they open no member gap.
            directive if directive.starts_with("preproc_") => {}
            _ => {
                let (start, start_row) = attach
                    .take()
                    .unwrap_or((child.start_byte(), child.start_position().row));
                let documented =
                    doc_row.is_some() || named_child_of_kind(child, "attribute_list").is_some();
                let gap_doc_row = doc_row.take();
                let gap_conditional = conditional;
                conditional = false;

                let prev_end = prev.as_ref().map_or(body.start_byte(), |p| p.end);
                let gap = &source[prev_end..start];
                if let Some(p) = prev.as_ref()
                    && (documented || p.documented)
                    && gap.contains('\n')
                    && !has_blank_line(gap)
                {
                    out.push(gap_diagnostic(
                        kind,
                        &declared_name(node, source),
                        source,
                        MemberGap {
                            prev: p.node,
                            next: child,
                            doc_row: gap_doc_row,
                            conditional: gap_conditional,
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

/// The item's declared name.
fn declared_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("name")?
        .utf8_text(source.as_bytes())
        .ok()
        .map(str::to_string)
}

/// Builds the warning for one unseparated member gap.
///
/// The diagnostic line anchors at the next member's first doc comment,
/// or at its first attached node when it carries none. The finding then
/// sits where the blank line belongs.
///
/// A gap holding a conditional anchors at the `#if` row instead: the
/// blank line clears the finding only above the conditional.
fn gap_diagnostic(
    kind: &'static str,
    item_name: &Option<String>,
    source: &str,
    gap: MemberGap<'_>,
) -> Diagnostic {
    let a = member_label(gap.prev, source);
    let b = member_label(gap.next, source);
    let item = item_name.as_deref().unwrap_or(kind);
    let suggestion = if gap.conditional {
        "Add one blank line before the `#if` conditional, keeping all content\n  unchanged."
            .to_string()
    } else {
        format!(
            "Add one blank line between `{a}` and `{b}`, before any docs and\n  attributes of `{b}`."
        )
    };
    Diagnostic {
        title: Some("missing blank line between documented members".into()),
        severity: Severity::Warning,
        code: CODE_FMT001,
        message: format!(
            "`{item}` {kind} members `{a}` and `{b}` need a blank line between them.\n\
             Why: a blank line keeps each doc comment attached to its own member.\n\
             Suggestions:\n\
             - {suggestion}"
        ),
        line: if gap.conditional {
            gap.next_start_row + 1
        } else {
            gap.doc_row.unwrap_or(gap.next_start_row) + 1
        },
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

/// True when the comment node is a doc comment (`///` or `/**`).
fn is_doc_comment(node: tree_sitter::Node<'_>, source: &str) -> bool {
    node.utf8_text(source.as_bytes())
        .is_ok_and(|text| text.starts_with("///") || text.starts_with("/**"))
}

/// The diagnostic kind for one member-list item kind.
///
/// The member noun is always `members`. Items without a member body
/// return `None`.
fn item_shape(kind: &str) -> Option<&'static str> {
    match kind {
        "class_declaration" => Some("class"),
        "struct_declaration" => Some("struct"),
        "interface_declaration" => Some("interface"),
        "record_declaration" => Some("record"),
        "enum_declaration" => Some("enum"),
        _ => None,
    }
}

/// A member's declared name, or a phrase for its node kind.
///
/// Fields and event fields carry no `name` field of their own, so their
/// label comes from the declared variable's name.
fn member_label(member: tree_sitter::Node<'_>, source: &str) -> String {
    if let Some(name) = member
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
    {
        return name.to_string();
    }
    declared_field_name(member, source)
        .unwrap_or_else(|| member_kind_phrase(member.kind()).to_string())
}

/// The variable name of a field or event field declaration, if any.
fn declared_field_name(member: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    if !matches!(
        member.kind(),
        "field_declaration" | "event_field_declaration"
    ) {
        return None;
    }
    let declaration = named_child_of_kind(member, "variable_declaration")?;
    let declarator = named_child_of_kind(declaration, "variable_declarator")?;
    declarator
        .child_by_field_name("name")?
        .utf8_text(source.as_bytes())
        .ok()
        .map(str::to_string)
}

/// Diagnostic phrase for a member node without a usable name.
fn member_kind_phrase(kind: &str) -> &'static str {
    match kind {
        "method_declaration" => "method",
        "property_declaration" => "property",
        "constructor_declaration" => "constructor",
        "destructor_declaration" => "destructor",
        "field_declaration" => "field",
        "event_field_declaration" => "event field",
        "indexer_declaration" => "indexer",
        "operator_declaration" => "operator",
        "delegate_declaration" => "delegate",
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

    /// Parses `source` and runs FMT001 over it.
    fn checks(source: &str) -> Vec<Diagnostic> {
        check(&parse(source).expect("fixture parses"))
    }

    // ── documented gaps without blank lines ──

    #[rstest]
    #[case::class(
        r#"/// An edge in a flow graph.
public class Edge
{
    /// <summary>The source address.</summary>
    public int Source;
    /// <summary>The target address.</summary>
    public int Target;
}
"#,
        "class",
        "Edge",
        "Source",
        "Target",
        6
    )]
    #[case::structure(
        r#"/// A value pair.
public struct Pair
{
    /// <summary>The low half.</summary>
    public int Low;
    /// <summary>The high half.</summary>
    public int High;
}
"#,
        "struct",
        "Pair",
        "Low",
        "High",
        6
    )]
    #[case::interface(
        r#"/// Reads bytes.
public interface IReader
{
    /// <summary>Reads the first byte.</summary>
    int First();
    /// <summary>Reads the last byte.</summary>
    int Last();
}
"#,
        "interface",
        "IReader",
        "First",
        "Last",
        6
    )]
    #[case::record(
        r#"/// A located value.
public record Spot
{
    /// <summary>The row.</summary>
    public int Row { get; init; }
    /// <summary>The column.</summary>
    public int Column { get; init; }
}
"#,
        "record",
        "Spot",
        "Row",
        "Column",
        6
    )]
    #[case::enumeration(
        r#"/// How control leaves an instruction.
public enum Exit
{
    /// <summary>Follows the branch.</summary>
    Taken,
    /// <summary>Falls through.</summary>
    NotTaken,
}
"#,
        "enum",
        "Exit",
        "Taken",
        "NotTaken",
        6
    )]
    fn check_should_flag_documented_gap_for_every_item_kind(
        #[case] source: &str,
        #[case] kind: &str,
        #[case] name: &str,
        #[case] first: &str,
        #[case] second: &str,
        #[case] line: usize,
    ) {
        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_FMT001);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[0].item_kind, kind);
        assert_eq!(diags[0].item_name.as_deref(), Some(name));
        assert!(diags[0].message.contains(&format!("`{first}`")));
        assert!(diags[0].message.contains(&format!("`{second}`")));
        assert_eq!(diags[0].line, line);
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
    fn check_should_flag_when_only_one_member_is_documented(
        #[case] source: &str,
        #[case] line: usize,
    ) {
        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, line);
    }

    #[test]
    fn check_should_explain_the_fix_when_class_members_are_documented() {
        let source = r#"class S
{
    /// <summary>d for a.</summary>
    int a;
    /// <summary>d for b.</summary>
    int b;
}
"#;

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
        assert_eq!(
            diags[0].message,
            indoc! {"
                `S` class members `a` and `b` need a blank line between them.
                Why: a blank line keeps each doc comment attached to its own member.
                Suggestions:
                - Add one blank line between `a` and `b`, before any docs and
                  attributes of `b`."
            }
        );
    }

    // ── exemptions: no room, no docs, no gap ──

    #[rstest]
    #[case::blank_line_between_documented(
        r#"class S
{
    /// <summary>d for a.</summary>
    int a;

    /// <summary>d for b.</summary>
    int b;
}
"#
    )]
    #[case::undocumented_members(
        r#"class P
{
    int x;
    int y;
}
"#
    )]
    #[case::documented_members_share_a_line(
        indoc! {"
            class P {
                /// <summary>d for x.</summary>
                int x; int y;
            }
        "}
    )]
    #[case::empty_body(indoc! {"
        class E {
        }
    "})]
    #[case::single_documented_member(
        r#"class S
{
    /// <summary>d for a.</summary>
    int a;
}
"#
    )]
    #[case::positional_record("public record Spot(int Row, int Column);\n")]
    fn check_should_stay_quiet_when_members_are_exempt(#[case] source: &str) {
        assert!(checks(source).is_empty());
    }

    // ── gap details: anchors, comments, conditionals ──

    #[test]
    fn check_should_anchor_at_first_doc_line_when_docs_wrap() {
        let source = indoc! {"
            class S {
                int a;
                /// First line.
                /// Second line.
                int b;
            }
        "};

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn check_should_flag_when_a_plain_comment_sits_between_members() {
        let source = indoc! {"
            class S {
                /// d for a.
                int a;
                // note.
                /// d for b.
                int b;
            }
        "};

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }

    #[test]
    fn check_should_flag_when_a_trailing_comment_shares_the_prev_member_row() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a; // note
                /// d for b.
                int b;
            }
        "};

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }

    #[test]
    fn check_should_anchor_at_the_member_when_only_a_trailing_comment_precedes_it() {
        let source = indoc! {"
            class C {
                /// d for a.
                int a; // note
                int b;
            }
        "};

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }

    #[test]
    fn check_should_flag_every_documented_gap_in_one_body() {
        let source = r#"class S
{
    /// d for a.
    int a;
    /// d for b.
    int b;
    /// d for c.
    int c;
}
"#;

        let lines: Vec<usize> = checks(source).iter().map(|d| d.line).collect();

        assert_eq!(lines, [5, 7]);
    }

    #[test]
    fn check_should_flag_members_of_nested_types() {
        let source = r#"class Outer
{
    /// d for inner.
    class Inner
    {
        /// d for x.
        int x;
        /// d for y.
        int y;
    }
}
"#;

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].item_name.as_deref(), Some("Inner"));
        assert_eq!(diags[0].line, 8);
    }

    #[test]
    fn check_should_stay_quiet_for_members_inside_preprocessor_conditionals() {
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

        assert!(checks(source).is_empty());
    }

    #[rstest]
    #[case::whitespace_only_line(
        "class S {\n    /// d for a.\n    int a;\n    \n    /// d for b.\n    int b;\n}\n"
    )]
    #[case::crlf_blank_line(
        "class S {\r\n    /// d for a.\r\n    int a;\r\n\r\n    /// d for b.\r\n    int b;\r\n}\r\n"
    )]
    fn check_should_stay_quiet_when_the_blank_line_has_whitespace(#[case] source: &str) {
        assert!(checks(source).is_empty());
    }

    #[test]
    fn check_should_anchor_at_the_conditional_when_it_sits_between_members() {
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

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
        assert!(diags[0].message.contains("`#if` conditional"));
    }

    #[test]
    fn check_should_stay_quiet_when_a_region_directive_sits_between_members() {
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

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`a` and `b`"));
        assert_eq!(diags[0].line, 5);
    }

    #[test]
    fn check_should_anchor_at_the_doc_when_a_comment_precedes_the_conditional() {
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

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 8);
        assert!(!diags[0].message.contains("`#if` conditional"));
    }

    #[test]
    fn check_should_flag_when_line_endings_are_crlf() {
        let source = "class S {\r\n    /// d for a.\r\n    int a;\r\n    /// d for b.\r\n    int b;\r\n}\r\n";

        let diags = checks(source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }
}
