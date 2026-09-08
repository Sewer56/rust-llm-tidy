//! Import collection from `using` directives.

use super::names::name_segments;
use super::scope::Import;
use super::syntax::{leaf_text, leftmost_name};
use tree_sitter::Node;

/// The explicit imports of one `using` directive.
///
/// A plain directive opens its namespace and binds nothing; its
/// `short` (the last segment) still feeds the hint's name and the
/// mention checks. An aliased directive (`using X = A.B.C;`)
/// binds the alias.
///
/// Static imports cannot supply named advice without type checking.
pub(super) fn collect_usings<'a>(bytes: &'a [u8], node: Node) -> Vec<Import<'a>> {
    let mut imports = Vec::new();
    if is_static_using(node) {
        return imports;
    }

    let alias = node.child_by_field_name("name");
    let alias_id = alias.map(|alias| alias.id());
    for i in 0..node.named_child_count() as u32 {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        if Some(child.id()) == alias_id
            || !matches!(
                child.kind(),
                "identifier" | "generic_name" | "qualified_name" | "alias_qualified_name"
            )
        {
            continue;
        }
        let Some(segments) = name_segments(child, bytes) else {
            break;
        };
        let bound = alias.and_then(|alias| leaf_text(alias, bytes));
        let short = bound.unwrap_or_else(|| *segments.last().expect("using path has a name"));
        imports.push(Import {
            segments,
            short,
            aliased: bound.is_some(),
            absolute: leftmost_name(child).is_some_and(|n| n.kind() == "alias_qualified_name"),
        });
        break;
    }
    imports
}

/// True when `node` is a `using static` directive: its anonymous
/// children carry the `static` modifier token.
fn is_static_using(node: Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| !child.is_named() && child.kind() == "static")
}
