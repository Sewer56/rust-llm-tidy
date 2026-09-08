//! Classification of tree-sitter nodes into reordering categories.
//!
//! Maps each top-level item node to an [`ItemKind`], extracts its name and
//! impl target, derives the visibility tier, captures its leading doc comments,
//! and (for functions) whether it returns `Result`. These classifications feed
//! the parse orchestration that builds source items.
//!
//! # Children
//!
//! - `trivia`: pending attachable trivia, doc-comment extraction, and the
//!   attribute reads behind `#[test]`/`#[cfg(test)]` detection.
//! - `signature`: visibility tiers, parameter names, `Result` return-type
//!   facts, and the type-path/identifier predicates they reduce to.
//!
//! # Re-exports
//!
//! Consumers import every entry point from this module, never from the
//! child paths: [`classify_item`], [`PendingTrivia`], and the
//! trivia/signature reads.
//!
//! The shared node-reading primitives (`child_of_kind`, `text_of`, ...)
//! also live here and stay private to this module tree.

pub(super) use self::signature::result_error_type;
use self::signature::{
    classify_visibility, extract_param_names, field_ident_text, find_macro_invocation,
    first_ident_of_type, last_path_segment, returns_result,
};
pub(super) use self::trivia::{PendingTrivia, is_attachable, is_transparent_comment};
use self::trivia::{collect_attributes, extract_doc_comments, is_test_fn, is_test_module};
pub(in crate::languages::rust) use self::trivia::{doc_attribute_content, is_outer_doc};
use crate::source::{ItemKind, VisibilityTier};
use tree_sitter::Node;

mod signature;
mod trivia;

/// Result of classifying a single top-level item.
pub(super) struct Classification {
    pub(super) kind: ItemKind,
    pub(super) name: Option<String>,
    pub(super) impl_target: Option<String>,
    pub(super) is_test_module: bool,
    pub(super) is_inline: bool,
    pub(super) is_trait_impl: bool,
    pub(super) visibility: Option<VisibilityTier>,
    pub(super) doc_comments: Vec<String>,
    pub(super) returns_result: bool,
    /// Named parameter idents of a fn, excluding `self`/`&self`/`&mut self`.
    /// Empty for non-fn items.
    pub(super) params: Vec<String>,
    /// True for fn items carrying a `#[test]` or `#[...::test]` attribute.
    pub(super) is_test_fn: bool,
}

/// Classify a top-level item node into a [`Classification`].
///
/// `body` is the item node itself (e.g. `function_item`).
///
/// `pending` holds the attachable trivia (attrs + outer docs) immediately
/// preceding it, used for doc-comment extraction and `#[test]`/`#[cfg(test)]`
/// detection. `source` is the full source text for text extraction.
pub(super) fn classify_item<'a>(
    body: Node<'a>,
    source: &str,
    pending: &PendingTrivia<'a>,
) -> Classification {
    let doc_comments = extract_doc_comments(&pending.nodes, source);
    let attrs = collect_attributes(&pending.nodes);
    match body.kind() {
        "function_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            returns_result: returns_result(body, source),
            params: extract_param_names(body, source),
            is_test_fn: is_test_fn(&attrs, source),
            ..base(ItemKind::Fn, doc_comments)
        },
        "struct_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Struct, doc_comments)
        },
        "enum_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Enum, doc_comments)
        },
        "union_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Union, doc_comments)
        },
        "type_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Type, doc_comments)
        },
        "impl_item" => Classification {
            impl_target: body
                .child_by_field_name("type")
                .and_then(|t| first_ident_of_type(t, source)),
            is_trait_impl: body.child_by_field_name("trait").is_some(),
            ..base(ItemKind::Impl, doc_comments)
        },
        "use_declaration" => Classification {
            visibility: classify_visibility(body),
            ..base(ItemKind::Use, doc_comments)
        },
        "const_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Const, doc_comments)
        },
        "static_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Static, doc_comments)
        },
        "mod_item" => Classification {
            name: field_ident_text(body, "name", source),
            is_test_module: is_test_module(&attrs, source),
            // True only when the mod has an inline `{ ... }` body (a
            // `declaration_list` `body` child); file-based `mod x;` has no body.
            is_inline: body.child_by_field_name("body").is_some(),
            visibility: classify_visibility(body),
            ..base(ItemKind::Mod, doc_comments)
        },
        "extern_crate_declaration" => Classification {
            visibility: classify_visibility(body),
            ..base(ItemKind::Extern, doc_comments)
        },
        "trait_item" => Classification {
            name: field_ident_text(body, "name", source),
            visibility: classify_visibility(body),
            ..base(ItemKind::Trait, doc_comments)
        },
        "macro_definition" => Classification {
            name: field_ident_text(body, "name", source),
            ..base(ItemKind::Macro, doc_comments)
        },
        // A top-level macro invocation may appear as a bare `macro_invocation`
        // node or wrapped in an `expression_statement` (`foo!();`).
        //
        // The body node passed in is whichever covers the full byte range;
        // locate the inner `macro_invocation` for the macro path.
        "macro_invocation" | "expression_statement" => {
            let mac = find_macro_invocation(body);
            Classification {
                name: mac.and_then(|m| {
                    m.child_by_field_name("macro")
                        .and_then(|p| last_path_segment(p, source))
                }),
                ..base(ItemKind::MacroInvocation, doc_comments)
            }
        }
        _ => base(ItemKind::Other, doc_comments),
    }
}

/// Classification carrying only `kind` and `doc_comments`, with every other
/// field at the value most item kinds use; classify arms override the rest.
fn base(kind: ItemKind, doc_comments: Vec<String>) -> Classification {
    Classification {
        kind,
        doc_comments,
        name: None,
        impl_target: None,
        is_test_module: false,
        is_inline: false,
        is_trait_impl: false,
        visibility: None,
        returns_result: false,
        params: Vec::new(),
        is_test_fn: false,
    }
}

/// First named child of `node` whose kind equals `kind`.
fn child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let count = node.named_child_count() as u32;
    (0..count).find_map(|i| {
        let c = node.named_child(i)?;
        (c.kind() == kind).then_some(c)
    })
}

/// Leftmost identifier `&str` of a path/type node.
fn first_segment<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    match node.kind() {
        "identifier" | "type_identifier" => node.utf8_text(source.as_bytes()).ok(),
        "scoped_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("path")
            .and_then(|p| first_segment(p, source)),
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(|t| first_segment(t, source)),
        _ => None,
    }
}

/// True when `node` has a child with field name `field`.
fn has_field(node: Node, field: &str) -> bool {
    node.child_by_field_name(field).is_some()
}

/// True when `node` has any named child whose kind satisfies `pred`.
fn named_child_exists(node: Node<'_>, pred: impl Fn(&str) -> bool) -> bool {
    let count = node.named_child_count() as u32;
    (0..count).any(|i| node.named_child(i).is_some_and(|c| pred(c.kind())))
}

/// Text of a node if it is a simple identifier-ish leaf.
fn text_of<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    if matches!(node.kind(), "identifier" | "type_identifier") {
        node.utf8_text(source.as_bytes()).ok()
    } else {
        None
    }
}
