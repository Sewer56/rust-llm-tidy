//! Extract invocation names and array shapes without resolving types or imports.

use crate::config::{ArrayKind, SymbolLanguage};
use core::ops::RangeInclusive;
use tree_sitter::Node;

/// One invocation's normalized name and legacy diagnostic spelling.
pub(super) struct Usage<'a> {
    pub(super) path: String,
    pub(super) array_kind: Option<ArrayKind>,
    pub(super) name_lines: RangeInclusive<usize>,
    pub(super) zero_arguments: bool,
    pub(super) no_initializer: bool,
    pub(super) kind: &'static str,
    pub(super) legacy_name: &'a str,
    pub(super) legacy_zero_arguments: bool,
}

/// Recognize one invocation; unrelated syntax produces no occurrence.
pub(super) fn extract<'a>(
    node: Node<'_>,
    source: &'a str,
    language: SymbolLanguage,
) -> Option<Usage<'a>> {
    let (name, kind) = match (language, node.kind()) {
        (SymbolLanguage::Rust, "call_expression") => {
            let mut name = node.child_by_field_name("function")?;
            if name.kind() == "generic_function" {
                name = name.child_by_field_name("function")?;
            }
            if name.kind() == "field_expression" {
                name = name.child_by_field_name("field")?;
            }
            (name, "call")
        }
        (SymbolLanguage::Rust, "macro_invocation") => (node.child_by_field_name("macro")?, "macro"),
        (SymbolLanguage::Csharp, "object_creation_expression") => {
            (node.child_by_field_name("type")?, "creation")
        }
        (SymbolLanguage::Csharp, "invocation_expression") => {
            (node.child_by_field_name("function")?, "call")
        }
        (
            SymbolLanguage::Csharp,
            "array_creation_expression" | "implicit_array_creation_expression",
        ) => {
            return Some(array_creation(node));
        }
        _ => return None,
    };
    let (mut path, name_lines) = written_path(name, source)?;
    if kind == "macro" {
        path.push('!');
    } else if kind == "creation" {
        path.push_str("::new");
    }

    let legacy_name = legacy_name(node, name, source, language, kind);
    let zero_arguments = node.child_by_field_name("arguments").is_some_and(|args| {
        let mut cursor = args.walk();
        !args
            .named_children(&mut cursor)
            .any(|child| !child.is_extra())
    });
    Some(Usage {
        path,
        array_kind: None,
        name_lines,
        zero_arguments,
        no_initializer: node.child_by_field_name("initializer").is_none(),
        kind,
        legacy_name,
        legacy_zero_arguments: if kind == "macro" {
            true
        } else {
            node.child_by_field_name("arguments")
                .is_some_and(|args| args.named_child_count() == 0)
        },
    })
}

/// Describe array syntax in the shared traversal; size expressions are not names.
fn array_creation(node: Node<'_>) -> Usage<'static> {
    let mut cursor = node.walk();
    let no_initializer = !node
        .named_children(&mut cursor)
        .any(|child| child.kind() == "initializer_expression");
    let vector = node.child_by_field_name("type").is_some_and(|ty| {
        let element = ty.child_by_field_name("type");
        let rank = ty.child_by_field_name("rank");
        element.is_some_and(|element| element.kind() != "array_type")
            && rank.is_some_and(|rank| {
                let mut cursor = rank.walk();
                let mut children = rank.children(&mut cursor);
                let mut sizes = 0;
                for child in &mut children {
                    if child.kind() == "," {
                        return false;
                    }
                    if child.is_named() && !child.is_extra() {
                        sizes += 1;
                    }
                }
                sizes == 1
            })
    });

    let line = node.start_position().row + 1;
    Usage {
        path: "new[]".to_owned(),
        array_kind: Some(if vector {
            ArrayKind::ExplicitSizedVector
        } else {
            ArrayKind::Any
        }),
        name_lines: line..=line,
        zero_arguments: true,
        no_initializer,
        kind: "array creation",
        legacy_name: "new[]",
        legacy_zero_arguments: true,
    }
}

/// Retain the old PERF001 spelling even where SYM001 normalizes generics.
fn legacy_name<'a>(
    node: Node<'_>,
    name: Node<'_>,
    source: &'a str,
    language: SymbolLanguage,
    kind: &str,
) -> &'a str {
    let original = if kind == "call" {
        node.child_by_field_name("function").unwrap_or(name)
    } else {
        name
    };
    let original = match (language, original.kind()) {
        (SymbolLanguage::Rust, "field_expression") => {
            original.child_by_field_name("field").unwrap_or(original)
        }
        (SymbolLanguage::Csharp, "member_access_expression") => {
            original.child_by_field_name("name").unwrap_or(original)
        }
        _ => original,
    };
    let text = source.get(original.byte_range()).unwrap_or_default().trim();
    if kind == "creation" {
        text.split('<')
            .next()
            .unwrap_or(text)
            .rsplit('.')
            .next()
            .unwrap_or(text)
            .trim()
    } else {
        text
    }
}

/// Build a component path and the inclusive lines containing its name tokens.
/// Generic arguments and complex receiver expressions contribute no components.
fn written_path(node: Node<'_>, source: &str) -> Option<(String, RangeInclusive<usize>)> {
    let mut path = String::with_capacity(node.end_byte() - node.start_byte());
    let mut first_line = None;
    let mut last_line = 0;
    let mut cursor = node.walk();

    loop {
        let current = cursor.node();
        let descend = match current.kind() {
            "identifier" | "type_identifier" | "field_identifier" | "self" | "super" | "crate"
            | "predefined_type" => {
                if !path.is_empty() {
                    path.push_str("::");
                }
                path.push_str(source.get(current.byte_range())?);
                first_line.get_or_insert(current.start_position().row + 1);
                last_line = current.end_position().row + 1;
                false
            }
            "scoped_identifier"
            | "scoped_type_identifier"
            | "generic_type"
            | "generic_function"
            | "qualified_name"
            | "alias_qualified_name"
            | "generic_name"
            | "member_access_expression" => true,
            _ => false,
        };
        if descend && cursor.goto_first_child() {
            continue;
        }

        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return first_line.map(|first| (path, first..=last_line));
            }
        }
    }
}
