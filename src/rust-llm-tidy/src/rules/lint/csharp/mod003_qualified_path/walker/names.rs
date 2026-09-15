//! Dotted-chain segment reading and the names declarations and patterns bind.

use super::syntax::leaf_text;
use tree_sitter::Node;

/// The variable names a field or event-field declaration declares.
pub(super) fn declarator_names<'a>(bytes: &'a [u8], node: Node) -> Vec<&'a str> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declaration" {
            continue;
        }
        let mut declarators = child.walk();
        for declarator in child.children(&mut declarators) {
            if declarator.kind() == "variable_declarator"
                && let Some(name) = declarator
                    .child_by_field_name("name")
                    .and_then(|name| leaf_text(name, bytes))
            {
                names.push(name);
            }
        }
    }
    names
}

/// The names `node` binds, each with the byte offset it counts from.
///
/// Local designations (declarations, parameters, out-arguments,
/// foreach and catch names, pattern designations, query variables)
/// bind from their position onward.
pub(super) fn local_bindings<'a>(bytes: &'a [u8], node: Node) -> Vec<(&'a str, usize)> {
    let mut names: Vec<(&'a str, usize)> = Vec::new();
    match node.kind() {
        "parameter" | "catch_declaration" | "type_parameter" => {
            names.extend(field_name_at(bytes, node, node.start_byte()));
        }
        // An out-argument (`out var t`) or a positional pattern's
        // sub-designation (`Pair(int a, int t)`).
        "declaration_expression" | "from_clause" => {
            names.extend(field_name_binding(bytes, node));
        }
        // Identifier children bind as names: a query's `into`
        // continuation, a `join ... into t` continuation, and a
        // parenthesized designation (`var (a, t)`).
        //
        // The `into` identifier sits between clauses without its
        // own node; nested designations recurse by the walk.
        "query_expression" | "join_into_clause" | "parenthesized_variable_designation" => {
            names.extend(identifier_children(bytes, node));
        }
        // `let t = ...` carries its name as the leading identifier
        // child, ahead of the value expression.
        "let_clause" => {
            names.extend(identifier_children(bytes, node).first().copied());
        }
        // A `join t in ys` clause's range variable is the first
        // identifier after the clause's optional type; later
        // identifiers are the source and references.
        "join_clause" => {
            let type_id = node.child_by_field_name("type").map(|ty| ty.id());
            for i in 0..node.named_child_count() as u32 {
                let Some(child) = node.named_child(i) else {
                    continue;
                };
                if Some(child.id()) == type_id {
                    continue;
                }
                if child.kind() == "identifier"
                    && let Some(name) = leaf_text(child, bytes)
                {
                    names.push((name, child.start_byte()));
                }
                break;
            }
        }
        "variable_declarator" => {
            names.extend(field_name_at(bytes, node, node.start_byte()));
            // A deconstruction declaration (`var (a, b) = ...`)
            // binds its pattern names instead of a declarator name.
            for i in 0..node.named_child_count() as u32 {
                if let Some(child) = node.named_child(i)
                    && child.kind() == "tuple_pattern"
                {
                    names.extend(
                        tuple_pattern_names(bytes, child)
                            .into_iter()
                            .map(|name| (name, node.start_byte())),
                    );
                }
            }
        }
        // An `is` or `case` pattern designation (`o is int t`)
        // binds its name from the pattern onward.
        "declaration_pattern" => {
            names.extend(field_name_at(bytes, node, node.start_byte()));
        }
        // A simple lambda's parameter is its own aliased identifier
        // node, so the text is the name.
        "implicit_parameter" => {
            names.extend(
                node.utf8_text(bytes)
                    .ok()
                    .map(|name| (name, node.start_byte())),
            );
        }
        "foreach_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                names.extend(
                    designation_names(bytes, left)
                        .into_iter()
                        .map(|name| (name, node.start_byte())),
                );
            }
        }
        _ => {}
    }
    names
}

/// The dot-joined segments of a qualified-name chain, root first.
///
/// Handles both shapes a dotted path takes: `qualified_name` in
/// type positions and `member_access_expression` in expression
/// positions.
///
/// Returns `None` when the chain does not bottom out in plain
/// identifiers or `global::`. Generic chains are exempt rather than losing
/// type arguments in advice; non-global aliases never establish full paths.
pub(super) fn name_segments<'a>(node: Node, bytes: &'a [u8]) -> Option<Vec<&'a str>> {
    let mut segments = Vec::new();
    let mut current = node;
    loop {
        match current.kind() {
            "qualified_name" => {
                let name = current.child_by_field_name("name")?;
                if name.kind() != "identifier" {
                    return None;
                }
                segments.push(leaf_text(name, bytes)?);
                current = current.child_by_field_name("qualifier")?;
            }
            "member_access_expression" => {
                let name = current.child_by_field_name("name")?;
                if name.kind() != "identifier" {
                    return None;
                }
                segments.push(leaf_text(name, bytes)?);
                current = current.child_by_field_name("expression")?;
            }
            "alias_qualified_name" => {
                if leaf_text(current.child_by_field_name("alias")?, bytes)? != "global" {
                    return None;
                }
                current = current.child_by_field_name("name")?;
            }
            "identifier" => {
                segments.push(leaf_text(current, bytes)?);
                break;
            }
            _ => return None,
        }
    }
    segments.reverse();
    Some(segments)
}

/// The names a foreach designation binds: a simple name, a
/// destructuring pattern, or a declaration's variable.
fn designation_names<'a>(bytes: &'a [u8], node: Node) -> Vec<&'a str> {
    match node.kind() {
        "identifier" => leaf_text(node, bytes).into_iter().collect(),
        "tuple_pattern" => tuple_pattern_names(bytes, node),
        "declaration" | "variable_declaration" => {
            let mut names = Vec::new();
            for i in 0..node.named_child_count() as u32 {
                if let Some(child) = node.named_child(i) {
                    names.extend(designation_names(bytes, child));
                }
            }
            names
        }
        "variable_declarator" => node
            .child_by_field_name("name")
            .and_then(|name| leaf_text(name, bytes))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// The name `node` binds through its `name` field, positioned at
/// `offset`.
fn field_name_at<'a>(bytes: &'a [u8], node: Node, offset: usize) -> Option<(&'a str, usize)> {
    node.child_by_field_name("name")
        .and_then(|n| leaf_text(n, bytes))
        .map(|name| (name, offset))
}

/// The name `node` binds through its `name` field, positioned at
/// the name itself.
fn field_name_binding<'a>(bytes: &'a [u8], node: Node) -> Option<(&'a str, usize)> {
    let name = node.child_by_field_name("name")?;
    leaf_text(name, bytes).map(|text| (text, name.start_byte()))
}

/// The direct identifier children of `node`, each bound at the
/// identifier itself.
fn identifier_children<'a>(bytes: &'a [u8], node: Node) -> Vec<(&'a str, usize)> {
    let mut names = Vec::new();
    for i in 0..node.named_child_count() as u32 {
        if let Some(child) = node.named_child(i)
            && child.kind() == "identifier"
            && let Some(name) = leaf_text(child, bytes)
        {
            names.push((name, child.start_byte()));
        }
    }
    names
}

/// The names a destructuring pattern binds, including nested
/// patterns.
fn tuple_pattern_names<'a>(bytes: &'a [u8], node: Node) -> Vec<&'a str> {
    let mut names = Vec::new();
    for i in 0..node.named_child_count() as u32 {
        if let Some(child) = node.named_child(i) {
            match child.kind() {
                "identifier" => names.extend(leaf_text(child, bytes)),
                "tuple_pattern" => names.extend(tuple_pattern_names(bytes, child)),
                _ => {}
            }
        }
    }
    names
}
