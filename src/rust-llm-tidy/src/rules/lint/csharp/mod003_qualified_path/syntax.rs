//! Tree-shape predicates and name-leaf readers for the qualified-name rule.

use tree_sitter::Node;

/// Detect conditional regions before reporting any path in a containing method.
pub(super) fn contains_conditional(node: Node) -> bool {
    if node.kind().starts_with("preproc_if") {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(contains_conditional)
}

/// True when `node` is the outermost node of a dotted name chain, so
/// it carries the whole path.
///
/// Inner chain links are covered by their parent: an expression
/// receiver such as `System.Console` inside
/// `System.Console.WriteLine` is a link, never a second occurrence,
/// so call-target receivers are never double-counted.
pub(super) fn is_chain_head(node: Node) -> bool {
    matches!(node.kind(), "qualified_name" | "member_access_expression")
        && !node.parent().is_some_and(|parent| {
            matches!(parent.kind(), "qualified_name" | "member_access_expression")
        })
}

/// Node kinds that introduce a lexical scope: the file, namespace
/// and type bodies, method-like declarations with parameters,
/// closures, loops, and catch clauses.
pub(super) fn is_scope(kind: &str) -> bool {
    matches!(
        kind,
        "compilation_unit"
            | "declaration_list"
            | "block"
            | "method_declaration"
            | "constructor_declaration"
            | "local_function_statement"
            | "lambda_expression"
            | "anonymous_method_expression"
            | "for_statement"
            | "foreach_statement"
            | "catch_clause"
    )
}

/// The leftmost segment of a dotted name chain, without building the
/// whole segment list.
pub(super) fn leftmost_segment<'a>(node: Node, bytes: &'a [u8]) -> Option<&'a str> {
    leaf_text(leftmost_name(node)?, bytes)
}

/// Text of a name leaf node (an identifier, or a `generic_name`
/// without its type arguments), or `None` for any other kind.
pub(super) fn leaf_text<'a>(node: Node, bytes: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(bytes).ok(),
        "generic_name" => (0..node.named_child_count() as u32)
            .filter_map(|i| node.named_child(i))
            .find(|child| child.kind() == "identifier")
            .and_then(|identifier| identifier.utf8_text(bytes).ok()),
        _ => None,
    }
}

/// Locate the root syntax without mistaking an alias for an ordinary identifier.
pub(super) fn leftmost_name<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
    let mut current = node;
    loop {
        match current.kind() {
            "qualified_name" => current = current.child_by_field_name("qualifier")?,
            "member_access_expression" => current = current.child_by_field_name("expression")?,
            "identifier" | "alias_qualified_name" => return Some(current),
            _ => return None,
        }
    }
}
