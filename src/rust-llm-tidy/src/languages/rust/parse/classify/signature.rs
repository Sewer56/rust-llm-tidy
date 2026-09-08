//! Signature-level facts for item classification: visibility, parameters,
//! and `Result` return types.
//!
//! Also hosts the reads behind those facts and item naming: name-field
//! text, impl-target first segments, macro path segments, and the `Result`
//! error-argument predicates.

use super::{child_of_kind, first_segment, named_child_exists, text_of};
use crate::source::VisibilityTier;
use tree_sitter::Node;

/// Classify a visibility modifier child of an item node into a tier.
///
/// `body` is the item node; it may have a `visibility_modifier` child. `pub`
/// alone -> [`VisibilityTier::Pub`]; `pub(crate)`/`pub(super)`/`pub(in path)`
/// (any restriction) -> [`VisibilityTier::PubRestricted`]; no modifier ->
/// [`VisibilityTier::Private`] (inherited).
///
/// Note: tree-sitter-rust exposes `visibility_modifier` as a *child* of the
/// item node (not a named field), so it is located by kind, not field name.
pub(super) fn classify_visibility(body: Node<'_>) -> Option<VisibilityTier> {
    let Some(vis) = child_of_kind(body, "visibility_modifier") else {
        return Some(VisibilityTier::Private);
    };
    // The modifier has a `pub` named child; any *other* named child
    // (`crate`/`super`/`self`/identifier/scoped_identifier) is a restriction.
    let restricted = named_child_exists(vis, |k| k != "pub");
    if restricted {
        Some(VisibilityTier::PubRestricted)
    } else {
        Some(VisibilityTier::Pub)
    }
}

/// Extract named parameter idents from a `function_item`'s parameters, excluding
/// the implicit `self`/`&self`/`&mut self` receiver.
///
/// Only simple `identifier` patterns are reported (the common case);
/// destructuring patterns contribute nothing.
pub(super) fn extract_param_names(body: Node, source: &str) -> Vec<String> {
    let Some(params) = body.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let count = params.named_child_count() as u32;
    let mut out = Vec::new();
    for i in 0..count {
        let Some(child) = params.named_child(i) else {
            continue;
        };
        if child.kind() != "parameter" {
            continue;
        }
        if let Some(pat) = child.child_by_field_name("pattern")
            && pat.kind() == "identifier"
            && let Ok(name) = pat.utf8_text(source.as_bytes())
        {
            out.push(name.to_string());
        }
    }
    out
}

/// Text of an identifier/`type_identifier` child found by field name.
pub(super) fn field_ident_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    text_of(child, source).map(str::to_string)
}

/// Find the `macro_invocation` child of a node, if any. Used to unwrap a
/// top-level macro invocation wrapped in an `expression_statement`.
pub(super) fn find_macro_invocation(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "macro_invocation" {
        return Some(node);
    }
    let count = node.named_child_count() as u32;
    (0..count).find_map(|i| {
        let c = node.named_child(i)?;
        (c.kind() == "macro_invocation").then_some(c)
    })
}

/// The leftmost identifier text of a type/path node, mirroring syn's
/// `path_type_to_string` (first segment only).
///
/// Descends through `generic_type`, `scoped_type_identifier`, and
/// `scoped_identifier`.
pub(super) fn first_ident_of_type(node: Node<'_>, source: &str) -> Option<String> {
    first_segment(node, source).map(str::to_string)
}

/// Last path segment identifier of a macro path (for invocation naming).
pub(super) fn last_path_segment(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => node.utf8_text(source.as_bytes()).ok().map(str::to_string),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(str::to_string),
        _ => None,
    }
}

/// Final path segment of the `Result` return type's error argument.
///
/// `None` when:
///
/// - `body` has no return type.
/// - The return type is not a generic type with at least two arguments.
/// - The error argument is not a plain path (tuple, array, reference).
///
/// The caller gates on [`returns_result`].
///
/// Qualified error paths pointing at other modules also yield `None`:
///
/// - `std::io::Error` cannot name a same-file enum, even when its final
///   segment collides with one.
/// - Only a single crate-root prefix (`crate::Error`, `self::Error`,
///   `super::Error`) can still resolve to a top-level enum.
pub(in super::super) fn result_error_type(body: Node<'_>, source: &str) -> Option<String> {
    let rt = body.child_by_field_name("return_type")?;
    if rt.kind() != "generic_type" {
        return None;
    }
    let args = rt.child_by_field_name("type_arguments")?;
    if args.named_child_count() < 2 {
        return None;
    }
    let error = args.named_child(1)?;
    let base = type_path_root(error);
    if is_scoped_type(base) && !is_direct_crate_root(base, source) {
        return None;
    }
    last_type_segment(error, source).map(str::to_string)
}

/// True when `sig` (a `function_item`) declares a `-> Result<...>` return type.
///
/// Matches by the final path segment name so any `Result` (std, io, a custom
/// error result, etc.) is detected regardless of path prefix or generic args.
pub(super) fn returns_result(body: Node<'_>, source: &str) -> bool {
    let Some(rt) = body.child_by_field_name("return_type") else {
        return false;
    };
    last_type_segment(rt, source) == Some("Result")
}

/// True when a scoped type's prefix is exactly one crate-root segment
/// (`crate::Error`), so the final segment can still name a same-file
/// top-level enum.
///
/// Longer prefixes (`crate::nested::Error`, `std::io::Error`) point at
/// other modules and are rejected.
fn is_direct_crate_root(node: Node<'_>, source: &str) -> bool {
    let path = node.child_by_field_name("path");
    let Some(path) = path else {
        return false;
    };
    if !matches!(path.kind(), "type_identifier" | "identifier") {
        return false;
    }
    matches!(
        path.utf8_text(source.as_bytes()).ok(),
        Some("crate") | Some("self") | Some("super")
    )
}

/// True for path-qualified type nodes (`std::io::Error`).
fn is_scoped_type(node: Node<'_>) -> bool {
    matches!(node.kind(), "scoped_type_identifier" | "scoped_identifier")
}

/// Last path-segment identifier of a type node, or `None` for non-path types
/// (`&T`, `[T; n]`, etc.) - mirroring syn, which only matched `Type::Path`.
fn last_type_segment<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    match node.kind() {
        "type_identifier" => node.utf8_text(source.as_bytes()).ok(),
        "scoped_type_identifier" | "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok()),
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(|t| last_type_segment(t, source)),
        _ => None,
    }
}

/// Root path node of a type node, unwrapping generic wrappers
/// (`Vec<T>` -> `T`) so qualification can be inspected.
fn type_path_root<'a>(node: Node<'a>) -> Node<'a> {
    match node.kind() {
        "generic_type" => node
            .child_by_field_name("type")
            .map_or(node, type_path_root),
        _ => node,
    }
}
