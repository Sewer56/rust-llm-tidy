//! `use`-declaration import collection and missing-import advice.

use super::scope::Import;
use super::syntax::{leaf_text, scoped_segments};
use tree_sitter::Node;

/// The explicit imports and glob prefixes of one `use` declaration.
///
/// Groups flatten to one import per member (`use a::{B, C}`
/// imports `a::B` and `a::C`); an alias binds the alias name. Glob
/// prefixes return separately to mark uncertain root resolution.
pub(super) fn collect_use<'a>(bytes: &'a [u8], node: Node) -> (Vec<Import<'a>>, Vec<Vec<&'a str>>) {
    let mut imports = Vec::new();
    let mut globs = Vec::new();
    if let Some(argument) = node.child_by_field_name("argument") {
        collect_use_argument(bytes, argument, &[], &mut imports, &mut globs);
    }
    (imports, globs)
}

/// The `use` import that hoists `path`: the whole path, or its prefix
/// through the first uppercase-initial segment.
///
/// By Rust naming that segment names a type or trait. Every segment
/// after it is then an associated item, and `use` cannot import one:
/// rustc rejects `use std::sync::Arc::new;` (E0432).
///
/// This naming heuristic avoids suggesting an associated-item import.
///
/// A path of only lowercase segments names modules plus one importable
/// item and stays whole. Snake-case type names break the naming rule,
/// not this advice.
pub(super) fn use_path(path: &str) -> &str {
    let mut offset = 0;
    for segment in path.split("::") {
        if segment.starts_with(|c: char| c.is_ascii_uppercase()) {
            return &path[..offset + segment.len()];
        }
        offset += segment.len() + 2;
    }
    path
}

/// Collect imports from one `use` argument node under `prefix`.
fn collect_use_argument<'a>(
    bytes: &'a [u8],
    node: Node,
    prefix: &[&'a str],
    imports: &mut Vec<Import<'a>>,
    globs: &mut Vec<Vec<&'a str>>,
) {
    match node.kind() {
        // Bare path: `use std;`. `self` inside a group binds the
        // group's prefix path itself (`use a::b::{self}` imports
        // `a::b` as `b`).
        "identifier" | "crate" | "self" | "super" => {
            if let Some(text) = leaf_text(node, bytes) {
                if text == "self" && !prefix.is_empty() {
                    let short = prefix[prefix.len() - 1];
                    imports.push(Import {
                        segments: prefix.to_vec(),
                        short,
                    });
                } else {
                    push_import(imports, prefix, &[text], text);
                }
            }
        }
        "scoped_identifier" => {
            if let Some(segments) = scoped_segments(node, bytes) {
                let short = *segments.last().expect("scoped path has a name");
                push_import(imports, prefix, &segments, short);
            }
        }
        "use_as_clause" => {
            let alias = node
                .child_by_field_name("alias")
                .and_then(|n| leaf_text(n, bytes));
            if let (Some(alias), Some(segments)) = (
                alias,
                node.child_by_field_name("path")
                    .and_then(|p| path_argument_segments(bytes, p)),
            ) {
                push_import(imports, prefix, &segments, alias);
            }
        }
        "use_list" => {
            for i in 0..node.named_child_count() as u32 {
                if let Some(child) = node.named_child(i) {
                    collect_use_argument(bytes, child, prefix, imports, globs);
                }
            }
        }
        "scoped_use_list" => {
            if let (Some(prefix_path), Some(list)) = (
                node.child_by_field_name("path")
                    .and_then(|p| path_argument_segments(bytes, p)),
                node.child_by_field_name("list"),
            ) {
                let mut nested = prefix.to_vec();
                nested.extend(prefix_path.iter().copied());
                collect_use_argument(bytes, list, &nested, imports, globs);
            }
        }
        "use_wildcard" => {
            let mut glob = prefix.to_vec();
            if let Some(child) = node.named_child(0)
                && let Some(segments) = path_argument_segments(bytes, child)
            {
                glob.extend(segments.iter().copied());
            }
            globs.push(glob);
        }
        _ => {}
    }
}

/// The segments of a `use` argument path node: a scoped chain or a
/// bare leaf (`crate`, `self`, `super`, an identifier).
fn path_argument_segments<'a>(bytes: &'a [u8], node: Node) -> Option<Vec<&'a str>> {
    match node.kind() {
        "scoped_identifier" => scoped_segments(node, bytes),
        "identifier" | "crate" | "self" | "super" => leaf_text(node, bytes).map(|t| vec![t]),
        _ => None,
    }
}

/// Append `inner` under `prefix` as one import binding `short`.
fn push_import<'a>(
    imports: &mut Vec<Import<'a>>,
    prefix: &[&'a str],
    inner: &[&'a str],
    short: &'a str,
) {
    let mut segments = prefix.to_vec();
    segments.extend(inner.iter().copied());
    imports.push(Import { segments, short });
}
