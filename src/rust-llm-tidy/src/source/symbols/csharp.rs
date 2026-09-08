//! Read C# type/member names, retaining shared multi-field declaration spans.

use super::{Declaration, declaration, path};
use tree_sitter::Node;

/// Collect lexical declarations, including all preprocessor branches.
pub(super) fn collect(container: Node<'_>, parent: &str, source: &str, out: &mut Vec<Declaration>) {
    let mut namespace = None;
    let mut cursor = container.walk();
    for node in container.named_children(&mut cursor) {
        let parent = namespace.as_deref().unwrap_or(parent);
        if node.kind() == "file_scoped_namespace_declaration" {
            if let Some(name) = node.child_by_field_name("name") {
                namespace = Some(path(parent, &namespace_name(name, source)));
            }
            continue;
        }
        if node.kind().starts_with("preproc_") {
            collect(node, parent, source, out);
            continue;
        }

        if matches!(node.kind(), "field_declaration" | "event_field_declaration") {
            collect_fields(node, parent, source, out);
            continue;
        }
        if !matches!(
            node.kind(),
            "namespace_declaration"
                | "class_declaration"
                | "struct_declaration"
                | "interface_declaration"
                | "record_declaration"
                | "enum_declaration"
                | "delegate_declaration"
                | "method_declaration"
                | "constructor_declaration"
                | "destructor_declaration"
                | "property_declaration"
                | "event_declaration"
                | "enum_member_declaration"
        ) {
            continue;
        }
        let Some(name) = node.child_by_field_name("name") else {
            continue;
        };
        let qualified = if node.kind() == "namespace_declaration" {
            path(parent, &namespace_name(name, source))
        } else {
            path(parent, &source[name.byte_range()])
        };
        if let Some(body) = node.child_by_field_name("body")
            && matches!(
                body.kind(),
                "declaration_list" | "enum_member_declaration_list"
            )
        {
            collect(body, &qualified, source, out);
        }
        out.push(declaration(node, name, qualified, source));
    }
}

/// Give every field name the entire declaration, including attributes and docs.
fn collect_fields(node: Node<'_>, parent: &str, source: &str, out: &mut Vec<Declaration>) {
    let mut cursor = node.walk();
    let Some(variables) = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "variable_declaration")
    else {
        return;
    };

    let mut cursor = variables.walk();
    for variable in variables.named_children(&mut cursor) {
        if variable.kind() == "variable_declarator"
            && let Some(name) = variable.child_by_field_name("name")
        {
            out.push(declaration(
                node,
                name,
                path(parent, &source[name.byte_range()]),
                source,
            ));
        }
    }
}

/// Use the same path separator for block and file-scoped namespace syntax.
fn namespace_name(node: Node<'_>, source: &str) -> String {
    source[node.byte_range()]
        .split_whitespace()
        .collect::<String>()
        .replace('.', "::")
}
