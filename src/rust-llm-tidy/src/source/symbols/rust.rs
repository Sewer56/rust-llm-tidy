//! Read Rust declaration containers without imposing the reorder member model.

use super::{Declaration, declaration, path};
use tree_sitter::Node;

/// Collect named declarations in one lexical container, excluding expressions.
pub(super) fn collect(container: Node<'_>, parent: &str, source: &str, out: &mut Vec<Declaration>) {
    let mut cursor = container.walk();
    for node in container.named_children(&mut cursor) {
        if node.kind() == "impl_item" {
            if let Some(target) = node.child_by_field_name("type")
                && let Some(name) = target_path(target, source)
            {
                let qualified = path(parent, &name);
                if let Some(body) = node.child_by_field_name("body") {
                    collect(body, &qualified, source, out);
                }
                out.push(declaration(node, target, qualified, source));
            }
            continue;
        }

        if !matches!(
            node.kind(),
            "mod_item"
                | "struct_item"
                | "enum_item"
                | "union_item"
                | "trait_item"
                | "function_item"
                | "function_signature_item"
                | "field_declaration"
                | "enum_variant"
                | "const_item"
                | "static_item"
                | "type_item"
                | "associated_type"
                | "macro_definition"
                | "extern_crate_declaration"
        ) {
            if node.kind() == "foreign_mod_item"
                && let Some(body) = node.child_by_field_name("body")
            {
                collect(body, parent, source, out);
            }
            continue;
        }
        let Some(name) = node.child_by_field_name("name") else {
            continue;
        };
        let qualified = path(parent, &source[name.byte_range()]);
        if matches!(
            node.kind(),
            "mod_item" | "struct_item" | "enum_item" | "union_item" | "trait_item" | "enum_variant"
        ) && let Some(body) = node.child_by_field_name("body")
        {
            collect(body, &qualified, source, out);
        }
        out.push(declaration(node, name, qualified, source));
    }
}

/// Remove generic arguments from a written impl target, but retain its path.
fn target_path(node: Node<'_>, source: &str) -> Option<Box<str>> {
    match node.kind() {
        "type_identifier" | "identifier" | "primitive_type" | "crate" | "self" | "super" => {
            Some(source[node.byte_range()].into())
        }
        "generic_type" => target_path(node.child_by_field_name("type")?, source),
        "scoped_type_identifier" | "scoped_identifier" => {
            let name = target_path(node.child_by_field_name("name")?, source)?;
            let prefix = node
                .child_by_field_name("path")
                .and_then(|prefix| target_path(prefix, source));
            Some(match prefix {
                Some(prefix) => path(&prefix, &name),
                None => format!("::{name}").into_boxed_str(),
            })
        }
        _ => None,
    }
}
