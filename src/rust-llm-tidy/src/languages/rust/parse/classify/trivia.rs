//! Trivia handling for classification: pending doc comments and attributes.
//!
//! Owns the pending run of attachable nodes captured while walking
//! top-level items: outer doc comments and `#[...]` attributes.
//!
//! Provides the attachment predicates that grow or skip that run, the
//! doc-comment extraction from it, and the attribute reads behind
//! `#[test]`/`#[cfg(test)]` detection.
//!
//! It also tracks the comment directly above the first attribute, which the
//! TEST002 rule reads as the item's summary comment.
//!
//! `doc_attribute_content` and `is_outer_doc` are shared beyond this
//! module tree: the parse module re-exports them for the language
//! module's doc-region producer.

use super::{child_of_kind, first_segment, has_field, text_of};
use tree_sitter::Node;

/// A pending run of attachable trivia (outer doc comments + attributes)
/// preceding an item, captured while walking the top-level nodes.
///
/// `doc` nodes are `///` (outer) line/block comments; `attr` nodes are
/// `#[...]` attribute items. Both attach to the following item.
#[derive(Default)]
pub(in super::super) struct PendingTrivia<'a> {
    pub(super) nodes: Vec<Node<'a>>,
    /// The first `#[...]` attribute item in the run, if any.
    first_attr: Option<Node<'a>>,
    /// The comment directly above [`Self::first_attr`], recorded when that
    /// attribute is pushed. `None` when the attribute opens the run.
    first_attr_comment: Option<Node<'a>>,
    /// The most recent comment node in the run, doc or transparent. Used when
    /// the item carries no attributes.
    last_comment: Option<Node<'a>>,
}

impl<'a> PendingTrivia<'a> {
    pub(in super::super) fn new() -> Self {
        Self {
            nodes: Vec::new(),
            first_attr: None,
            first_attr_comment: None,
            last_comment: None,
        }
    }

    pub(in super::super) fn push(&mut self, node: Node<'a>) {
        // Freeze the summary candidate at the first attribute: later comments
        // sit below the attribute block and cannot qualify.
        if node.kind() == "attribute_item" && self.first_attr.is_none() {
            self.first_attr = Some(node);
            self.first_attr_comment = self.last_comment;
        }

        if is_comment(node) {
            self.last_comment = Some(node);
        }

        self.nodes.push(node);
    }

    /// Record a transparent comment as the most recent comment in the run.
    ///
    /// Plain `//` comments and inner `//!` docs are transparent to
    /// attachment; a nearer one supersedes any doc above it.
    pub(in super::super) fn note_comment(&mut self, node: Node<'a>) {
        if is_comment(node) {
            self.last_comment = Some(node);
        }
    }

    /// True when a comment sits directly above the item's first attribute (or
    /// its body, when it carries no attributes).
    ///
    /// Accepts any line or block comment on the line directly above the
    /// boundary: outer docs (`///`/`/** */`), inner docs (`//!`/`/*! */`),
    /// and plain `//`/`/* */` comments.
    ///
    /// The last line of a multi-line run counts. A blank line between the
    /// comment and the boundary breaks the run.
    pub(in super::super) fn has_summary_comment(&self, body: Node<'a>, source: &str) -> bool {
        let (comment, target) = match self.first_attr {
            Some(attr) => (self.first_attr_comment, attr),
            None => (self.last_comment, body),
        };

        comment.is_some_and(|c| comment_adjacent(c, target, source))
    }

    /// Byte offset of the first attachable trivia node, i.e. the item's
    /// "syn_start" (start of its leading attrs/docs), used for `start_line`.
    pub(in super::super) fn attached_start(&self) -> Option<usize> {
        self.nodes.first().map(|n| n.start_byte())
    }
}

/// Collect the `attribute` nodes from pending `attribute_item` trivia, in
/// source order. Used for `#[test]` / `#[cfg(test)]` detection.
///
/// The `attribute` is a *child* of `attribute_item` (not a named field in
/// tree-sitter-rust), so it is located by kind.
pub(super) fn collect_attributes<'a>(trivia: &[Node<'a>]) -> Vec<Node<'a>> {
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
///   preserves the leading space (e.g. ` foo`).
/// - The trailing newline (part of the `doc_comment` node for line comments)
///   is stripped to match syn's `#[doc = " foo"]` value semantics.
/// - `#[doc = "..."]` attributes: the literal's `string_content` text (sans
///   surrounding quotes) is the value syn stores for the attribute form.
///   A `#[doc = " foo"]` line yields ` foo` - identical to the `/// foo`
///   form.
///
/// List-form `#[doc(...)]` (e.g. `#[doc(hidden)]`) and non-`doc` attributes
/// are not doc-comment lines; they are still collected by `collect_attributes`
/// for `#[test]`/`#[cfg(test)]` detection but contribute no doc text here.
pub(super) fn extract_doc_comments(trivia: &[Node], source: &str) -> Vec<String> {
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
                if let Some(content) = doc_attribute_content(*node, source)
                    && let Ok(text) = content.utf8_text(source.as_bytes())
                {
                    docs.push(text.to_string());
                }
            }
            _ => {}
        }
    }
    docs
}

/// True when `node` is attachable leading trivia: an outer doc comment
/// (`///`) or an attribute item (`#[...]`).
///
/// Inner docs (`//!`) and plain comments (`//`) are NOT attachable; they are
/// transparent to attachment.
pub(in super::super) fn is_attachable(node: Node) -> bool {
    match node.kind() {
        "attribute_item" => true,
        "line_comment" | "block_comment" => is_outer_doc(node),
        _ => false,
    }
}

/// True when the attrs carry a test-marker attribute.
///
/// Matching the last path segment covers both the bare attribute and its
/// framework-scoped spelling: `#[test]`/`#[tokio::test]`, `#[rstest]`/
/// `#[path::rstest]`, and `#[test_case]`/`#[path::test_case]`.
pub(super) fn is_test_fn(attrs: &[Node<'_>], source: &str) -> bool {
    attrs.iter().any(|a| {
        attr_last_segment(*a, source).is_some_and(|s| matches!(s, "test" | "rstest" | "test_case"))
    })
}

/// True when the attrs contain a `#[cfg(test)]` attribute (exactly `cfg(test)`).
///
/// Mirrors syn's strict `tokens == "test"` check: the `cfg` attribute with a
/// `token_tree` argument containing exactly one `test` identifier and nothing
/// else.
pub(super) fn is_test_module(attrs: &[Node<'_>], source: &str) -> bool {
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

/// True when `node` is a non-attachable comment: a plain `//`/`/* */` or an
/// inner doc `//!`/`/** ! */`.
///
/// These are transparent to attachment (neither
/// attach to an item nor break the pending run of attachable trivia).
pub(in super::super) fn is_transparent_comment(node: Node) -> bool {
    if matches!(node.kind(), "line_comment" | "block_comment") {
        !is_outer_doc(node)
    } else {
        // `empty_statement` and `shebang` nodes are transparent (ignored):
        // they neither attach nor break attachment.
        //
        // Stray top-level statements are handled by `collect_item_entries`
        // instead.
        matches!(node.kind(), "empty_statement" | "shebang")
    }
}

/// Extract the `string_content` node of a `#[doc = "..."]` attribute
/// item: the literal's text between its quotes, positioned where the
/// text starts in the file.
///
/// Shared by doc-comment extraction here and the language module's Rust
/// doc-region producer, which reads the value's lines from the node's
/// text and start row.
///
/// Returns `None` when `item` is not a doc attribute with a single
/// string value:
///
/// - a list form (`#[doc(hidden)]`) or any attribute named other than
///   `doc`,
/// - a scoped path (`#[path::doc = "..."]`),
/// - a value that is not one string literal.
///
/// # Arguments
///
/// - `item` - the `attribute_item` node to read.
/// - `source` - the full source text for text extraction.
pub(in super::super::super) fn doc_attribute_content<'a>(
    item: Node<'a>,
    source: &str,
) -> Option<Node<'a>> {
    let attr = child_of_kind(item, "attribute")?;
    // The attribute path must be exactly `doc` (a plain identifier, not scoped).
    let path = attr_path(attr)?;
    if path.kind() != "identifier" || path.utf8_text(source.as_bytes()).ok()? != "doc" {
        return None;
    }
    // `#[doc = "..."]` carries the literal in the `value` field.
    // List forms like `#[doc(hidden)]` have an `arguments` `token_tree`
    // and are not doc-comment lines.
    let value = attr.child_by_field_name("value")?;
    if value.kind() != "string_literal" {
        return None;
    }
    child_of_kind(value, "string_content")
}

/// True when a `line_comment`/`block_comment` node is an OUTER doc comment
/// (`///` or `/** */`), i.e. it has an `outer` field.
///
/// Shared by attachment classification here and the language module's Rust
/// doc-region producer.
///
/// # Arguments
///
/// - `node` - the `line_comment` or `block_comment` node to test.
pub(in super::super::super) fn is_outer_doc(node: Node) -> bool {
    has_field(node, "outer")
}

/// First path segment of an `attribute` node's path (e.g. `cfg` in `#[cfg(...)]`).
fn attr_first_segment<'a>(attr: Node<'a>, source: &'a str) -> Option<&'a str> {
    attr_path(attr).and_then(|p| first_segment(p, source))
}

/// Last path segment of an `attribute` node's path (e.g. `test` in
/// `#[tokio::test]`).
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

/// True when `comment` ends on the line directly above `target`.
///
/// A line comment carries its trailing newline in the node, so its end
/// position already sits on the next line; a block comment does not.
fn comment_adjacent(comment: Node<'_>, target: Node<'_>, source: &str) -> bool {
    let end = comment.end_byte();
    let ends_with_newline = end > 0 && source.as_bytes()[end - 1] == b'\n';

    comment.end_position().row - usize::from(ends_with_newline) + 1 == target.start_position().row
}

/// True for a comment node of either spelling.
fn is_comment(node: Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

/// The attribute's path child (identifier/scoped_identifier/etc.).
fn attr_path(attr: Node<'_>) -> Option<Node<'_>> {
    let count = attr.named_child_count() as u32;
    (0..count).find_map(|i| {
        let c = attr.named_child(i)?;
        matches!(c.kind(), "identifier" | "scoped_identifier").then_some(c)
    })
}
