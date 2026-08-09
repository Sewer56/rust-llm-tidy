//! Classification of tree-sitter nodes into reordering categories.
//!
//! Maps each top-level item node to an [`ItemKind`], extracts its name and
//! impl target, derives the visibility tier, captures its leading doc comments,
//! and (for functions) whether it returns `Result`. These classifications feed
//! the parse orchestration that builds source items.

use crate::parse::item::{ItemKind, VisibilityTier};
use tree_sitter::Node;

/// Result of classifying a single top-level item.
pub(super) struct Classification {
    pub(super) kind: ItemKind,
    pub(super) name: Option<String>,
    pub(super) impl_target: Option<String>,
    pub(super) is_test_module: bool,
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

/// A pending run of attachable trivia (outer doc comments + attributes)
/// preceding an item, captured while walking the top-level nodes.
///
/// `doc` nodes are `///` (outer) line/block comments; `attr` nodes are
/// `#[...]` attribute items. Both attach to the following item.
#[derive(Default)]
pub(super) struct PendingTrivia<'a> {
    pub(super) nodes: Vec<Node<'a>>,
}

impl<'a> PendingTrivia<'a> {
    pub(super) fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub(super) fn push(&mut self, node: Node<'a>) {
        self.nodes.push(node);
    }

    /// Byte offset of the first attachable trivia node, i.e. the item's
    /// "syn_start" (start of its leading attrs/docs), used for `start_line`.
    pub(super) fn attached_start(&self) -> Option<usize> {
        self.nodes.first().map(|n| n.start_byte())
    }
}

/// Classify a top-level item node into a [`Classification`].
///
/// `body` is the item node itself (e.g. `function_item`). `pending` holds the
/// attachable trivia (attrs + outer docs) immediately preceding it, used for
/// doc-comment extraction and `#[test]`/`#[cfg(test)]` detection. `source` is
/// the full source text for text extraction.
pub(super) fn classify_item<'a>(
    body: Node<'a>,
    source: &str,
    pending: &PendingTrivia<'a>,
) -> Classification {
    let doc_comments = extract_doc_comments(&pending.nodes, source);
    let attrs = collect_attributes(&pending.nodes);
    match body.kind() {
        "function_item" => Classification {
            kind: ItemKind::Fn,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: returns_result(body, source),
            params: extract_param_names(body, source),
            is_test_fn: is_test_fn(&attrs, source),
        },
        "struct_item" => Classification {
            kind: ItemKind::Struct,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "enum_item" => Classification {
            kind: ItemKind::Enum,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "union_item" => Classification {
            kind: ItemKind::Union,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "type_item" => Classification {
            kind: ItemKind::Type,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "impl_item" => Classification {
            kind: ItemKind::Impl,
            name: None,
            impl_target: body
                .child_by_field_name("type")
                .and_then(|t| first_ident_of_type(t, source)),
            is_test_module: false,
            is_trait_impl: body.child_by_field_name("trait").is_some(),
            visibility: None,
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "use_declaration" => Classification {
            kind: ItemKind::Use,
            name: None,
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "const_item" => Classification {
            kind: ItemKind::Const,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "static_item" => Classification {
            kind: ItemKind::Static,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "mod_item" => Classification {
            kind: ItemKind::Mod,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: is_test_module(&attrs, source),
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "extern_crate_declaration" => Classification {
            kind: ItemKind::Extern,
            name: None,
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "trait_item" => Classification {
            kind: ItemKind::Trait,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: classify_visibility(body),
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        "macro_definition" => Classification {
            kind: ItemKind::Macro,
            name: field_ident_text(body, "name", source),
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: None,
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
        // A top-level macro invocation may appear as a bare `macro_invocation`
        // node or wrapped in an `expression_statement` (`foo!();`). The body
        // node passed in is whichever covers the full byte range; locate the
        // inner `macro_invocation` for the macro path.
        "macro_invocation" | "expression_statement" => {
            let mac = find_macro_invocation(body);
            Classification {
                kind: ItemKind::MacroInvocation,
                name: mac.and_then(|m| {
                    m.child_by_field_name("macro")
                        .and_then(|p| last_path_segment(p, source))
                }),
                impl_target: None,
                is_test_module: false,
                is_trait_impl: false,
                visibility: None,
                doc_comments,
                returns_result: false,
                params: Vec::new(),
                is_test_fn: false,
            }
        }
        _ => Classification {
            kind: ItemKind::Other,
            name: None,
            impl_target: None,
            is_test_module: false,
            is_trait_impl: false,
            visibility: None,
            doc_comments,
            returns_result: false,
            params: Vec::new(),
            is_test_fn: false,
        },
    }
}

/// True when `node` is attachable leading trivia: an outer doc comment
/// (`///`) or an attribute item (`#[...]`). Inner docs (`//!`) and plain
/// comments (`//`) are NOT attachable - they are transparent to attachment.
pub(super) fn is_attachable(node: Node) -> bool {
    match node.kind() {
        "attribute_item" => true,
        "line_comment" | "block_comment" => is_outer_doc(node),
        _ => false,
    }
}

/// True when `node` is a non-attachable comment: a plain `//`/`/* */` or an
/// inner doc `//!`/`/** ! */`. These are transparent to attachment (neither
/// attach to an item nor break the pending run of attachable trivia).
pub(super) fn is_transparent_comment(node: Node) -> bool {
    if matches!(node.kind(), "line_comment" | "block_comment") {
        !is_outer_doc(node)
    } else {
        // `empty_statement` and `shebang` nodes are treated as transparent
        // (ignored) so they neither attach nor break attachment. Stray
        // top-level statements are handled by `collect_item_entries` instead.
        matches!(node.kind(), "empty_statement" | "shebang")
    }
}

/// Classify a visibility modifier child of an item node into a tier.
///
/// `body` is the item node; it may have a `visibility_modifier` child. `pub`
/// alone -> [`VisibilityTier::Pub`]; `pub(crate)`/`pub(super)`/`pub(in path)`
/// (any restriction) -> [`VisibilityTier::PubRestricted`]; no modifier ->
/// [`VisibilityTier::Private`] (inherited).
///
/// Note: tree-sitter-rust exposes `visibility_modifier` as a *child* of the
/// item node (not a named field), so it is located by kind, not field name.
fn classify_visibility(body: Node<'_>) -> Option<VisibilityTier> {
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

/// Collect the `attribute` nodes from pending `attribute_item` trivia, in
/// source order. Used for `#[test]` / `#[cfg(test)]` detection.
///
/// The `attribute` is a *child* of `attribute_item` (not a named field in
/// tree-sitter-rust), so it is located by kind.
fn collect_attributes<'a>(trivia: &[Node<'a>]) -> Vec<Node<'a>> {
    trivia
        .iter()
        .copied()
        .filter(|n| n.kind() == "attribute_item")
        .filter_map(|n| child_of_kind(n, "attribute"))
        .collect()
}

/// Extract the text of each outer doc comment from the pending trivia nodes,
/// in source order.
///
/// Covers both equivalent spellings of an outer doc line:
///
/// - `/// foo` / `/** foo */` comments: the `doc` field child (`doc_comment`)
///   preserves the leading space (e.g. ` foo`). The trailing newline (part of
///   the `doc_comment` node for line comments) is stripped to match syn's
///   `#[doc = " foo"]` value semantics.
/// - `#[doc = "..."]` attributes: the literal's `string_content` text (sans
///   surrounding quotes) is the value syn stores for the attribute form, so a
///   `#[doc = " foo"]` line yields ` foo` - identical to the `/// foo` form.
///
/// List-form `#[doc(...)]` (e.g. `#[doc(hidden)]`) and non-`doc` attributes
/// are not doc-comment lines; they are still collected by `collect_attributes`
/// for `#[test]`/`#[cfg(test)]` detection but contribute no doc text here.
fn extract_doc_comments(trivia: &[Node], source: &str) -> Vec<String> {
    let mut docs = Vec::new();
    for node in trivia {
        match node.kind() {
            "line_comment" | "block_comment" => {
                if !is_outer_doc(*node) {
                    continue;
                }
                if let Some(doc) = node.child_by_field_name("doc")
                    && let Ok(text) = doc.utf8_text(source.as_bytes())
                {
                    // Line doc comments include the trailing newline in the
                    // `doc_comment` node; block docs do not. Strip trailing
                    // newlines.
                    docs.push(text.trim_end_matches(['\n', '\r']).to_string());
                }
            }
            "attribute_item" => {
                if let Some(attr) = child_of_kind(*node, "attribute")
                    && let Some(text) = doc_attribute_value(attr, source)
                {
                    docs.push(text);
                }
            }
            _ => {}
        }
    }
    docs
}

/// Extract named parameter idents from a `function_item`'s parameters, excluding
/// the implicit `self`/`&self`/`&mut self` receiver.
///
/// Only simple `identifier` patterns are reported (the common case);
/// destructuring patterns contribute nothing.
fn extract_param_names(body: Node, source: &str) -> Vec<String> {
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
fn field_ident_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    text_of(child, source).map(str::to_string)
}

/// Find the `macro_invocation` child of a node, if any. Used to unwrap a
/// top-level macro invocation wrapped in an `expression_statement`.
fn find_macro_invocation(node: Node<'_>) -> Option<Node<'_>> {
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
/// `path_type_to_string` (first segment only). Descends through
/// `generic_type`, `scoped_type_identifier`, and `scoped_identifier`.
fn first_ident_of_type(node: Node<'_>, source: &str) -> Option<String> {
    first_segment(node, source).map(str::to_string)
}

/// True when the attrs contain a `#[test]` or `#[...::test]` attribute.
///
/// Matching the last path segment covers both `#[test]` and framework variants
/// like `#[tokio::test]`.
fn is_test_fn(attrs: &[Node<'_>], source: &str) -> bool {
    attrs
        .iter()
        .any(|a| attr_last_segment(*a, source) == Some("test"))
}

/// True when the attrs contain a `#[cfg(test)]` attribute (exactly `cfg(test)`).
///
/// Mirrors syn's strict `tokens == "test"` check: the `cfg` attribute with a
/// `token_tree` argument containing exactly one `test` identifier and nothing
/// else.
fn is_test_module(attrs: &[Node<'_>], source: &str) -> bool {
    attrs.iter().any(|a| {
        // Path must be exactly `cfg`.
        if attr_first_segment(*a, source) != Some("cfg") {
            return false;
        }
        // The argument `token_tree` must contain exactly one identifier `test`.
        let Some(args) = a.child_by_field_name("arguments") else {
            return false;
        };
        if args.kind() != "token_tree" {
            return false;
        }
        let count = args.named_child_count() as u32;
        if count != 1 {
            return false;
        }
        args.named_child(0)
            .is_some_and(|c| c.kind() == "identifier" && text_of(c, source) == Some("test"))
    })
}

/// Last path segment identifier of a macro path (for invocation naming).
fn last_path_segment(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => node.utf8_text(source.as_bytes()).ok().map(str::to_string),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(str::to_string),
        _ => None,
    }
}

/// True when `sig` (a `function_item`) declares a `-> Result<...>` return type.
///
/// Matches by the final path segment name so any `Result` (std, io, a custom
/// error result, etc.) is detected regardless of path prefix or generic args.
fn returns_result(body: Node<'_>, source: &str) -> bool {
    let Some(rt) = body.child_by_field_name("return_type") else {
        return false;
    };
    last_type_segment(rt, source) == Some("Result")
}

/// First path segment of an `attribute` node's path (e.g. `cfg` in `#[cfg(...)]`).
fn attr_first_segment<'a>(attr: Node<'a>, source: &'a str) -> Option<&'a str> {
    attr_path(attr).and_then(|p| first_segment(p, source))
}

/// Last path segment of an `attribute` node's path (e.g. `test` in `#[tokio::test]`).
fn attr_last_segment<'a>(attr: Node<'a>, source: &'a str) -> Option<&'a str> {
    let path = attr_path(attr)?;
    match path.kind() {
        "identifier" => path.utf8_text(source.as_bytes()).ok(),
        "scoped_identifier" => path
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok()),
        _ => None,
    }
}

/// Extract the literal value of a `#[doc = "..."]` attribute node.
///
/// Returns the `string_literal`'s `string_content` (sans surrounding quotes),
/// mirroring syn's `#[doc = "..."]` value (so `#[doc = " foo"]` yields ` foo`,
/// matching `/// foo`). Returns `None` when `attr` is not an outer-doc
/// attribute - a list form (`#[doc(hidden)]`), a scoped path
/// (`#[path::doc = "..."]`), an attribute named something other than `doc`, or
/// an attribute whose value is not a single string literal.
fn doc_attribute_value(attr: Node<'_>, source: &str) -> Option<String> {
    // The attribute path must be exactly `doc` (a plain identifier, not scoped).
    let path = attr_path(attr)?;
    if path.kind() != "identifier" || path.utf8_text(source.as_bytes()).ok()? != "doc" {
        return None;
    }
    // `#[doc = "..."]` carries the literal in the `value` field; list forms
    // like `#[doc(hidden)]` instead have an `arguments` `token_tree` and are
    // not doc-comment lines.
    let value = attr.child_by_field_name("value")?;
    if value.kind() != "string_literal" {
        return None;
    }
    let content = child_of_kind(value, "string_content")?;
    content
        .utf8_text(source.as_bytes())
        .ok()
        .map(str::to_string)
}

/// True when a `line_comment`/`block_comment` node is an OUTER doc comment
/// (`///` or `/** */`), i.e. it has an `outer` field.
fn is_outer_doc(node: Node) -> bool {
    has_field(node, "outer")
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

/// The attribute's path child (identifier/scoped_identifier/etc.).
fn attr_path(attr: Node<'_>) -> Option<Node<'_>> {
    let count = attr.named_child_count() as u32;
    (0..count).find_map(|i| {
        let c = attr.named_child(i)?;
        matches!(c.kind(), "identifier" | "scoped_identifier").then_some(c)
    })
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
