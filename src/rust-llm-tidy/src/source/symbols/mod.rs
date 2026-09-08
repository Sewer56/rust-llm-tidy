//! Extract lexical declaration paths and owned source spans from retained trees.

use super::ParseResult;
use anyhow::{Result, ensure};
use core::ops::{Range, RangeInclusive};
use tree_sitter::Node;

mod csharp;
mod rust;
#[cfg(test)]
mod tests;

/// A named declaration, including its leading comments/attributes and full body.
pub(crate) struct Declaration {
    pub path: Box<str>,
    pub bytes: Range<usize>,
    /// One-based lines occupied by the declaration's name, not its documentation.
    pub name_lines: RangeInclusive<usize>,
}

/// Extract Rust or C# declarations without changing the retained syntax tree.
///
/// Paths use lexical `::` qualification for modules, namespaces, types and
/// members. C# namespace dots become `::`; Rust impl type arguments are omitted.
/// No aliases or receiver types are resolved.
///
/// An impl has the same path as its
/// written target type, so selecting a type also selects its impl bodies.
/// Each name in a C# field declaration owns the entire shared declaration.
///
/// Unsupported extensions return no declarations.
///
/// # Errors
///
/// Returns an error if a supported language's tree contains syntax errors or
/// missing tokens.
///
/// Callers must stop protected processing rather than treat an
/// incomplete declaration list as permission to edit.
pub(crate) fn declarations(parsed: &ParseResult, ext: &str) -> Result<Vec<Declaration>> {
    if !ext.eq_ignore_ascii_case("rs") && !ext.eq_ignore_ascii_case("cs") {
        return Ok(Vec::new());
    }
    let root = parsed.syntax_tree().root_node();
    ensure!(
        !root.has_error(),
        "cannot protect declarations in a syntax-error tree"
    );

    let mut declarations = Vec::new();
    if ext.eq_ignore_ascii_case("rs") {
        rust::collect(root, "", &parsed.source, &mut declarations);
    } else {
        csharp::collect(root, "", &parsed.source, &mut declarations);
    }
    Ok(declarations)
}

/// Retain the full declaration and adjacent leading standalone comments/attrs.
fn declaration(node: Node<'_>, name: Node<'_>, path: Box<str>, source: &str) -> Declaration {
    let mut first = node;
    while let Some(previous) = first.prev_named_sibling() {
        if !matches!(
            previous.kind(),
            "attribute_item" | "line_comment" | "block_comment" | "comment"
        ) || !source[previous.end_byte()..first.start_byte()]
            .trim()
            .is_empty()
        {
            break;
        }
        let text = &source[previous.byte_range()];
        if text.starts_with("//!") || text.starts_with("/*!") {
            break;
        }
        let line_start = previous.start_byte() - previous.start_position().column;
        if !source[line_start..previous.start_byte()].trim().is_empty() {
            break;
        }
        first = previous;
    }

    let line_start = first.start_byte() - first.start_position().column;
    let start = if source[line_start..first.start_byte()].trim().is_empty() {
        line_start
    } else {
        first.start_byte()
    };
    Declaration {
        path,
        bytes: start..node.end_byte(),
        name_lines: name.start_position().row + 1..=name.end_position().row + 1,
    }
}

/// Join a written name to its lexical container without semantic resolution.
fn path(parent: &str, name: &str) -> Box<str> {
    if parent.is_empty() {
        name.into()
    } else {
        format!("{parent}::{name}").into_boxed_str()
    }
}
