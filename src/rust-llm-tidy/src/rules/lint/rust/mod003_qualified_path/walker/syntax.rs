//! Tree-shape predicates and path-segment readers for the qualified-path rule.

use tree_sitter::Node;

/// Search a function once before emitting any hints, including nested bodies.
pub(super) fn contains_conditional(node: Node, bytes: &[u8]) -> bool {
    if is_conditional_attribute(node, bytes) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| contains_conditional(child, bytes))
}

/// Outer attributes are sibling nodes in the pinned Rust grammar.
pub(super) fn has_conditional_attribute(node: Node, bytes: &[u8]) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(attribute) = previous {
        if is_conditional_attribute(attribute, bytes) {
            return true;
        }
        if !matches!(
            attribute.kind(),
            "attribute_item" | "line_comment" | "block_comment"
        ) {
            break;
        }
        previous = attribute.prev_named_sibling();
    }
    false
}

/// True when `node` is the outermost node of a scoped path chain, so
/// it carries the whole path. Inner chain links are covered by their
/// parent.
pub(super) fn is_chain_head(node: Node) -> bool {
    matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier")
        && !node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "scoped_identifier" | "scoped_type_identifier"
            )
        })
}

/// Node kinds that introduce a lexical scope: blocks, module and
/// trait bodies, functions with their parameters, closures, loops, and
/// match arms.
pub(super) fn is_scope(kind: &str) -> bool {
    matches!(
        kind,
        "source_file"
            | "block"
            | "declaration_list"
            | "function_item"
            | "closure_expression"
            | "for_expression"
            | "match_arm"
    )
}

/// Path segments, with an empty first segment preserving a leading `::`.
///
/// Returns `None` when the chain does not bottom out in plain
/// segments (a generic or bracketed root such as
/// `<T as Trait>::Assoc`). The path text then cannot be decided
/// file-locally.
pub(super) fn scoped_segments<'a>(node: Node, bytes: &'a [u8]) -> Option<Vec<&'a str>> {
    let mut segments = Vec::new();
    let mut current = node;
    loop {
        let name = current.child_by_field_name("name")?;
        segments.push(leaf_text(name, bytes)?);
        match current.child_by_field_name("path") {
            None => {
                segments.push("");
                break;
            }
            Some(path) => match path.kind() {
                "identifier" | "type_identifier" | "crate" | "self" | "super" => {
                    segments.push(leaf_text(path, bytes)?);
                    break;
                }
                "scoped_identifier" | "scoped_type_identifier" => current = path,
                _ => return None,
            },
        }
    }
    segments.reverse();
    Some(segments)
}

/// Recognize real conditional attributes, never their text in comments or strings.
pub(super) fn is_conditional_attribute(node: Node, bytes: &[u8]) -> bool {
    matches!(node.kind(), "attribute_item" | "inner_attribute_item")
        && node
            .named_child(0)
            .and_then(|attr| attr.named_child(0))
            .is_some_and(|path| {
                path.kind() == "identifier"
                    && path
                        .utf8_text(bytes)
                        .is_ok_and(|text| matches!(text, "cfg" | "cfg_attr"))
            })
}

/// Text of a path leaf node (identifier, type identifier, or the
/// `crate`/`self`/`super` keywords), or `None` for any other kind.
pub(super) fn leaf_text<'a>(node: Node, bytes: &'a [u8]) -> Option<&'a str> {
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "crate" | "self" | "super"
    ) {
        node.utf8_text(bytes).ok()
    } else {
        None
    }
}
