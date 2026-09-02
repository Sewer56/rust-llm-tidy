//! C# parsing: tree-sitter-c-sharp sources into the shared item model.
//!
//! [`parse`] walks a `compilation_unit` and emits one [`SourceItem`]
//! per top-level declaration: using directives, namespaces, types,
//! preprocessor directives, statements.
//!
//! Namespace and type bodies additionally carry [`TypeMember`] lists so
//! the reorder engine can permute members.
//!
//! # Spans
//!
//! Spans follow the model crate's back-to-back layout: each item's
//! `end` is the byte after its trailing newline, and every non-first
//! item's `start` is the previous item's `end`.
//!
//! The blank lines and comments preceding an item travel with it.
//!
//! The first item's start extends over its own `///` doc-comment run
//! only; anything above lands in the preamble.
//!
//! # Members
//!
//! A body (namespace, class, struct, interface, record) emits members
//! only when its declaration list holds no preprocessor directive: the
//! grammar groups `#if`/`#else`/`#endif` runs into single `preproc_*`
//! nodes.
//!
//! Other directives (`#region`, `#define`, ...) are standalone nodes.
//!
//! A body carrying any of them is kept whole rather than permuted: a
//! directive must never move independently of the code it governs.
//!
//! # Regions
//!
//! Items and members carry the preprocessor region id of their
//! declaration line from one [`Regions`] scan.
//!
//! A scan that rejects the source leaves region `0` everywhere; the
//! reorder pass re-checks the scan and degrades to a no-op, so the
//! fallback never authorizes a move.

use super::lines::{end_past_newline, line_of, line_start_offsets, skip_one_line_ending};
use crate::regions::Regions;
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, SourceItem, TypeMember, VisibilityTier};

/// Attribute names marking a test method, per the accepted marker set; the
/// customary `Attribute` suffix is stripped before matching.
const TEST_MARKER_ATTRIBUTES: &[&str] = &["TestMethod", "Test", "Fact", "Theory"];

/// The declaration name of `node`, if it has a meaningful one.
///
/// Fields and event fields report their first declared variable (their
/// `variable_declaration` child carries no field name, so it is found by
/// kind); operators report their operator token; everything else reports
/// its `name` field.
pub(super) fn declaration_name(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let kind = node.kind();
    let text = |n: tree_sitter::Node<'_>| n.utf8_text(source.as_bytes()).unwrap_or("").to_string();
    if matches!(kind, "field_declaration" | "event_field_declaration") {
        let mut cursor = node.walk();
        let variable_declaration = node
            .children(&mut cursor)
            .find(|child| child.kind() == "variable_declaration")?;
        let mut declarators = variable_declaration.walk();
        let declarator = variable_declaration
            .children(&mut declarators)
            .find(|child| child.kind() == "variable_declarator")?;
        return declarator.child_by_field_name("name").map(text);
    }
    if kind == "operator_declaration" {
        return node.child_by_field_name("operator").map(text);
    }
    node.child_by_field_name("name").map(text)
}

/// The `///` doc-comment lines directly above `node`, in source order.
///
/// A doc run is the longest chain of `///` comment siblings where each
/// node sits on the line immediately above the next (no blank line
/// between), and the closest one sits directly above `node`.
///
/// Each entry keeps the text after `///` (so `/// Summary.` yields
/// `" Summary."`).
pub(super) fn doc_comment_texts(node: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    let Some(mut current) = doc_run_node(node, source) else {
        return Vec::new();
    };
    let mut docs = Vec::new();
    loop {
        let text = current.utf8_text(source.as_bytes()).unwrap_or("");
        if let Some(rest) = text.strip_prefix("///") {
            docs.push(rest.to_string());
        }
        match current.next_named_sibling() {
            Some(next) if next.kind() == "comment" => current = next,
            _ => break,
        }
    }
    docs
}

/// 1-based line of `node`'s `///` doc run start, if it has one.
pub(super) fn doc_run_start_line(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    doc_run_node(node, source).map(|run| run.start_position().row + 1)
}

/// True when `node` carries an attribute naming one of the accepted test
/// markers, matching the attribute name with its customary `Attribute`
/// suffix stripped.
pub(super) fn has_test_marker(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        child.is_named() && child.kind() == "attribute_list" && {
            let mut attrs = child.walk();
            child.children(&mut attrs).any(|attribute| {
                if !attribute.is_named() {
                    return false;
                }
                let Some(name) = attribute.child_by_field_name("name") else {
                    return false;
                };
                let Ok(text) = name.utf8_text(source.as_bytes()) else {
                    return false;
                };
                let bare = text.rsplit('.').next().unwrap_or(text);
                let bare = bare.strip_suffix("Attribute").unwrap_or(bare);
                TEST_MARKER_ATTRIBUTES
                    .iter()
                    .any(|marker| marker.eq_ignore_ascii_case(bare))
            })
        }
    })
}

/// Item kind of one declaration-list member.
///
/// Shared by the parse and the lint walk so members classify identically in
/// both.
pub(super) fn member_kind(kind: &str) -> ItemKind {
    match kind {
        "field_declaration" => ItemKind::Const,
        "event_declaration" | "event_field_declaration" => ItemKind::Event,
        "constructor_declaration" => ItemKind::Constructor,
        "destructor_declaration" => ItemKind::Destructor,
        "delegate_declaration" => ItemKind::Delegate,
        "enum_declaration" => ItemKind::Enum,
        "property_declaration" | "indexer_declaration" => ItemKind::Property,
        "operator_declaration" | "conversion_operator_declaration" => ItemKind::Operator,
        "method_declaration" => ItemKind::Fn,
        "using_directive" => ItemKind::Using,
        "namespace_declaration" => ItemKind::Namespace,
        "class_declaration" => ItemKind::Class,
        "struct_declaration" => ItemKind::Struct,
        "interface_declaration" => ItemKind::Interface,
        "record_declaration" => ItemKind::Record,
        _ => ItemKind::Other,
    }
}

/// Named parameter identifiers of a method, constructor, delegate, or
/// indexer declaration; empty for other declarations.
pub(super) fn parameter_names(node: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    let mut params = Vec::new();
    let Some(list) = node
        .child_by_field_name("parameters")
        .filter(|p| p.kind() == "parameter_list" || p.kind() == "bracketed_parameter_list")
    else {
        return params;
    };
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        if !child.is_named() || child.kind() != "parameter" {
            continue;
        }
        if let Some(name) = child
            .child_by_field_name("name")
            .map(|n| n.utf8_text(source.as_bytes()).unwrap_or("").to_string())
        {
            params.push(name);
        }
    }
    params
}

/// Parse a C# source file into the shared item model.
///
/// # Errors
///
/// Returns an error when the tree-sitter-c-sharp grammar cannot be
/// constructed, the parser rejects the language, or tree-sitter fails
/// to produce a syntax tree.
///
/// Syntactically invalid C# still parses (error-recovered); the reorder
/// pass declines such trees.
pub(super) fn parse(source: &str) -> anyhow::Result<ParseResult> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&super::c_sharp_language()?)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter parse returned no tree"))?;

    let line_starts = line_start_offsets(source);
    let regions = Regions::scan(source);
    // Collect the declaration nodes before moving the tree into the parse
    // result; the cursor borrows the tree it walks.
    let mut nodes = Vec::new();
    {
        let mut cursor = tree.root_node().walk();
        for node in tree.root_node().children(&mut cursor) {
            if node.is_named() && node.kind() != "comment" {
                nodes.push(node);
            }
        }
    }
    let mut items = Vec::new();
    for node in nodes {
        build_item(node, source, &line_starts, regions.as_ref(), &mut items);
    }

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

/// The visibility tier of `node` from its `modifier` children.
///
/// Explicit modifiers only: `public` maps to [`VisibilityTier::Pub`],
/// and `internal` plus the `protected` family (including
/// `protected internal` and `private protected`) to
/// [`VisibilityTier::PubRestricted`].
///
/// Explicit `private` and no modifier at all map to
/// [`VisibilityTier::Private`].
pub(super) fn visibility_of(node: tree_sitter::Node<'_>, source: &str) -> Option<VisibilityTier> {
    let mut saw_public = false;
    let mut saw_non_private = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() || child.kind() != "modifier" {
            continue;
        }
        match child.utf8_text(source.as_bytes()).unwrap_or("") {
            "public" => saw_public = true,
            "internal" | "protected" => saw_non_private = true,
            _ => {}
        }
    }
    Some(if saw_public {
        VisibilityTier::Pub
    } else if saw_non_private {
        VisibilityTier::PubRestricted
    } else {
        VisibilityTier::Private
    })
}

/// Build one top-level item for `node` and push it to `items`.
///
/// The item's span starts at the previous item's end (gap-anchored), except
/// the first item, which starts at its own `///` doc-comment run.
fn build_item(
    node: tree_sitter::Node<'_>,
    source: &str,
    line_starts: &[usize],
    regions: Option<&Regions>,
    items: &mut Vec<SourceItem>,
) {
    let decl_start = node.start_byte();
    let end = end_past_newline(node.end_byte(), line_starts, source.len());
    let attached = doc_run_start(node, source).unwrap_or(decl_start);
    let start = items.last().map_or(attached, |prev| prev.end);

    let kind = top_level_kind(node.kind());
    let body = body_list(node);
    let members = body
        .filter(|list| !has_preproc_child(*list))
        .map(|list| build_members(list, source, line_starts, regions))
        .unwrap_or_default();

    items.push(
        SourceItem::new(
            start,
            end,
            line_of(line_starts, attached),
            kind,
            declaration_name(node, source),
            None,
            false,
            false,
            false,
            visibility_of(node, source),
            doc_comment_texts(node, source),
            false,
            parameter_names(node, source),
            has_test_marker(node, source),
        )
        .with_region(region_of(regions, node))
        .with_members(members),
    );
}

/// The `declaration_list` body of a namespace or type declaration, if any.
fn body_list(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    node.child_by_field_name("body")
        .filter(|body| body.kind() == "declaration_list")
}

/// Build the member list of one `declaration_list` body, tiling spans
/// back-to-back: the first member starts right after the opening
/// brace's newline, and later members start at the previous member's
/// end.
///
/// Each member's end is the byte after its trailing newline.
///
/// A body whose members do not each occupy their own lines emits no
/// members instead: line-tiled spans cannot represent the body, so it
/// stays whole rather than permuting into a guessed rewrite.
///
/// That covers several members on one line, a first member sharing the
/// opening brace's row or preceded by any bytes on that row, and a
/// last member sharing the closing brace's line.
///
/// Blank lines and comments on their own rows after the brace stay
/// tileable: they travel with the first member.
fn build_members(
    list: tree_sitter::Node<'_>,
    source: &str,
    line_starts: &[usize],
    regions: Option<&Regions>,
) -> Vec<TypeMember> {
    let mut decls = Vec::new();
    let mut cursor = list.walk();
    for node in list.children(&mut cursor) {
        if !node.is_named() || node.kind() == "comment" {
            continue;
        }
        decls.push(node);
    }
    let shares_a_line = decls
        .windows(2)
        .any(|pair| pair[0].end_position().row >= pair[1].start_position().row)
        || decls.last().is_some_and(|last| {
            list.child(list.child_count().saturating_sub(1) as u32)
                .is_some_and(|brace| brace.start_position().row <= last.end_position().row)
        })
        || decls.first().is_some_and(|first| {
            list.child(0)
                .filter(|b| b.kind() == "{")
                .is_some_and(|brace| {
                    // Blank lines and indent after the brace's line
                    // ending tile fine (they travel with the first
                    // member); any bytes on the brace's own row do
                    // not.
                    let gap = &source[brace.end_byte()..first.start_byte()];
                    let trivia_before_newline = gap
                        .split_once('\n')
                        .is_some_and(|(before, _)| !before.chars().all(|c| c == '\r'));
                    brace.end_position().row >= first.start_position().row || trivia_before_newline
                })
        });
    if shares_a_line {
        return Vec::new();
    }

    let mut members = Vec::with_capacity(decls.len());
    for node in decls {
        let end = end_past_newline(node.end_byte(), line_starts, source.len());
        // The body's opening `{` is the list's first child; the first
        // member starts after its newline (or immediately when the body
        // opens mid-line).
        let start = members.last().map_or_else(
            || {
                let brace_end = list
                    .child(0)
                    .filter(|b| b.kind() == "{")
                    .map_or(list.start_byte(), |b| b.end_byte());
                skip_one_line_ending(brace_end, source)
            },
            |prev: &TypeMember| prev.end,
        );
        members.push(TypeMember::new(
            start,
            end,
            region_of(regions, node),
            member_kind(node.kind()),
            declaration_name(node, source),
        ));
    }
    members
}

/// Byte offset where `node`'s `///` doc run starts, if it has one.
fn doc_run_start(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    doc_run_node(node, source).map(|run| run.start_byte())
}

/// True when `list` holds any preprocessor directive node.
fn has_preproc_child(list: tree_sitter::Node<'_>) -> bool {
    let mut cursor = list.walk();
    list.children(&mut cursor)
        .any(|c| c.is_named() && c.kind().starts_with("preproc"))
}

/// Item kind of a top-level `compilation_unit` child.
fn top_level_kind(kind: &str) -> ItemKind {
    match kind {
        "using_directive" => ItemKind::Using,
        "namespace_declaration" | "file_scoped_namespace_declaration" => ItemKind::Namespace,
        "class_declaration" => ItemKind::Class,
        "struct_declaration" => ItemKind::Struct,
        "interface_declaration" => ItemKind::Interface,
        "record_declaration" => ItemKind::Record,
        "enum_declaration" => ItemKind::Enum,
        "delegate_declaration" => ItemKind::Delegate,
        _ => ItemKind::Other,
    }
}

/// The first `///` comment node of `node`'s doc run, if it has one.
///
/// A non-`///` comment ends the walk without discarding the run collected
/// so far: the run stays attached to the item while the plain comment
/// above it stays in the preamble (or in the previous item's span).
fn doc_run_node<'t>(node: tree_sitter::Node<'t>, source: &str) -> Option<tree_sitter::Node<'t>> {
    let mut current = node;
    while let Some(comment) = adjacent_comment_above(current, source) {
        let text = comment.utf8_text(source.as_bytes()).ok()?;
        if !text.starts_with("///") {
            break;
        }
        current = comment;
    }
    (current != node).then_some(current)
}

/// The preprocessor region id of `node`'s declaration line.
fn region_of(regions: Option<&Regions>, node: tree_sitter::Node<'_>) -> u32 {
    regions
        .map(|r| r.id_of_line(node.start_position().row + 1))
        .unwrap_or(0)
}

/// The comment sibling sitting on the line immediately above `node`, if any.
fn adjacent_comment_above<'t>(
    node: tree_sitter::Node<'t>,
    source: &str,
) -> Option<tree_sitter::Node<'t>> {
    let prev = node.prev_named_sibling()?;
    if prev.kind() != "comment" {
        return None;
    }
    let gap = &source[prev.end_byte()..node.start_byte()];
    is_adjacent_line(gap).then_some(prev)
}

/// True when `gap` is exactly one line ending plus horizontal whitespace,
/// so the two nodes sit on adjacent source lines.
fn is_adjacent_line(gap: &str) -> bool {
    let Some((before, after)) = gap.split_once('\n') else {
        return false;
    };
    before.chars().all(|c| c == '\r') && after.chars().all(|c| c == ' ' || c == '\t')
}
