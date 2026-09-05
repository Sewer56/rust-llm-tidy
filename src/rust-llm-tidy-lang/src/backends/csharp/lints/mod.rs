//! C# lint checks: the XML doc-comment dialect over the same codes and
//! [`Diagnostic`] shape the Rust checks emit.
//!
//! One module per rule, named by lint code: `doc001_missing_docs` through
//! `test001_test_naming`.
//!
//! [`run`] walks the compilation unit and every declaration list in
//! document order, collecting one [`Declaration`] fact set per
//! declaration.
//!
//! Every rule then runs over the collected facts in the same code order
//! the Rust backend emits (DOC001 through DOC006, then TEST001).
//!
//! The text checks (TEXT001, TEXT002) follow from the same parse's doc
//! regions.
//!
//! # Semantics
//!
//! - DOC001: non-private documentable declarations (`public`, `internal`,
//!   `protected`-family modifiers) need a `///` doc comment.
//! - DOC002: a non-private method or constructor whose body holds a
//!   `throw` needs an `<exception>` tag (error severity; the heuristic
//!   body scan can miss rethrows through helpers).
//! - DOC003: throwing members whose `<exception>` tags all lack a
//!   concrete `cref` type.
//! - DOC004: non-private methods, constructors, and indexers with
//!   parameters need `<param name="...">` tags.
//! - DOC005: `<param>` tags must name every declared parameter.
//! - DOC006: placeholder markers (`TODO`/`FIXME`/`TBD`) in doc comments.
//! - TEST001: `TestMethod`/`Test`/`Fact`/`Theory`-marked methods with
//!   discouraged (`test_*`, `case_*`, `test` + digits) names.
//! - TEXT001/TEXT002: `///` doc-comment prose measured with the XML doc
//!   dialect; findings carry original file lines. The dialect rules live
//!   with the lint crate's measuring core; see [`text_regions`]
//!   producer.
//!
//! [`text_regions`]: super::text_regions

use super::parse::{
    declaration_name, doc_comment_texts, doc_run_start_line, member_kind, parameter_names,
    visibility_of,
};
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, VisibilityTier};

mod doc001_missing_docs;
mod doc002_missing_exception_tag;
mod doc003_vague_exception;
mod doc004_missing_param_tags;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod test001_test_naming;

/// Kinds whose non-private declarations need doc comments.
const DOCUMENTABLE: &[ItemKind] = &[
    ItemKind::Class,
    ItemKind::Struct,
    ItemKind::Interface,
    ItemKind::Record,
    ItemKind::Enum,
    ItemKind::Delegate,
    ItemKind::Fn,
    ItemKind::Property,
    ItemKind::Event,
    ItemKind::Const,
    ItemKind::Static,
    ItemKind::Constructor,
];
/// Kinds checked for parameter documentation (DOC004/DOC005); properties
/// cover indexers, whose parameter lists hold real parameters.
const PARAMETERIZED: &[ItemKind] = &[ItemKind::Fn, ItemKind::Constructor, ItemKind::Property];
/// Kinds whose bodies are scanned for `throw` statements (DOC002/DOC003).
const THROWING: &[ItemKind] = &[ItemKind::Fn, ItemKind::Constructor];

/// One declaration's lint context: the shared facts every rule reads,
/// built once per declaration by the walker.
struct Declaration<'a> {
    /// The declaration's syntax node, for rules that walk attributes.
    node: tree_sitter::Node<'a>,
    /// The full source text, slicing companion to `node`.
    source: &'a str,
    /// The declaration's model kind.
    kind: ItemKind,
    /// The declaration's name, when it has a meaningful one.
    name: Option<String>,
    /// The declaration's `///` doc-comment lines.
    docs: Vec<String>,
    /// True when the visibility modifier is not `private`.
    non_private: bool,
    /// The 1-based diagnostic line: the `///` doc run's start when
    /// present, else the declaration's own row.
    line: usize,
    /// The `<exception>` tag facts for a non-private throwing member: tag
    /// count plus every `cref` value.
    ///
    /// `None` for members that cannot or do not throw, so DOC002 and
    /// DOC003 share one subtree walk.
    exception_scan: Option<(usize, Vec<String>)>,
    /// The declared parameter names paired with their `<param>` tag names,
    /// for a non-private parameterized member that declares parameters.
    ///
    /// `None` otherwise, so DOC004 and DOC005 share one parameter walk.
    param_scan: Option<(Vec<String>, Vec<String>)>,
}

impl Declaration<'_> {
    /// One diagnostic stamped with this declaration's line, kind, and
    /// name.
    fn diagnostic(&self, severity: Severity, code: &'static str, message: String) -> Diagnostic {
        Diagnostic {
            severity,
            code,
            message,
            line: self.line,
            item_kind: self.kind.as_str().to_string(),
            item_name: self.name.clone(),
        }
    }
}

/// Run every C# check over `parsed` and return all diagnostics in document
/// order: the declaration checks first, then the text checks (TEXT001,
/// TEXT002) over the same parse's doc regions.
///
/// Returns no diagnostics when the parse tree carries error nodes: a
/// broken tree would report findings against misread declarations, so the
/// whole pass degrades to silence.
pub(super) fn run(parsed: &ParseResult) -> Vec<Diagnostic> {
    if parsed.syntax_tree().root_node().has_error() {
        return Vec::new();
    }

    let source = parsed.source.as_str();
    let mut declarations = Vec::with_capacity(parsed.items.len());
    collect_children(parsed.syntax_tree().root_node(), source, &mut declarations);

    let mut diagnostics = Vec::with_capacity(parsed.items.len());
    for decl in &declarations {
        check_declaration(decl, &mut diagnostics);
    }

    diagnostics.extend(super::text_regions::text_checks(parsed));
    diagnostics
}

/// Run every rule over one collected declaration.
fn check_declaration(decl: &Declaration<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(doc001_missing_docs::check(decl));
    diagnostics.extend(doc002_missing_exception_tag::check(decl));
    diagnostics.extend(doc003_vague_exception::check(decl));
    diagnostics.extend(doc004_missing_param_tags::check(decl));
    diagnostics.extend(doc005_undocumented_param::check(decl));
    diagnostics.extend(doc006_placeholder::check(decl));
    diagnostics.extend(test001_test_naming::check(decl));
}

/// Collect the facts of every declaration under `list` (a
/// `compilation_unit` or `declaration_list`) in document order, recursing
/// into nested bodies and preprocessor branches.
fn collect_children<'a>(
    list: tree_sitter::Node<'a>,
    source: &'a str,
    declarations: &mut Vec<Declaration<'a>>,
) {
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        let kind = child.kind();
        if !child.is_named() || kind == "comment" {
            continue;
        }
        if let Some(body) = child
            .child_by_field_name("body")
            .filter(|b| b.kind() == "declaration_list")
        {
            collect_declaration(child, source, declarations);
            collect_children(body, source, declarations);
        } else if kind == "preproc_if" || kind == "preproc_else" || kind == "preproc_elif" {
            // Conditional branches hold real declarations; collect them.
            collect_children(child, source, declarations);
        } else {
            collect_declaration(child, source, declarations);
        }
    }
}

/// Collect one declaration node's facts into `declarations`.
///
/// Skips nodes that carry no member facts: usings, namespaces, and
/// unrecognized kinds.
fn collect_declaration<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a str,
    declarations: &mut Vec<Declaration<'a>>,
) {
    let kind = member_kind(node.kind());
    if kind == ItemKind::Other || kind == ItemKind::Using || kind == ItemKind::Namespace {
        return;
    }

    // Shared facts: computed once per declaration, never per rule.
    let non_private = visibility_of(node, source).is_some_and(|vis| vis != VisibilityTier::Private);
    let docs = doc_comment_texts(node, source);
    // The subtree walks run gated exactly as their rules require, so a
    // declaration pays for each scan at most once.
    let exception_scan = if non_private && THROWING.contains(&kind) && throws(node) {
        Some(exception_tags(&docs))
    } else {
        None
    };
    let param_scan = if non_private && PARAMETERIZED.contains(&kind) {
        let params = parameter_names(node, source);
        (!params.is_empty()).then(|| (params, param_tag_names(&docs)))
    } else {
        None
    };
    declarations.push(Declaration {
        node,
        source,
        kind,
        name: declaration_name(node, source),
        // The `///` doc run's line when present, else the declaration's own
        // row; doc_run_start_line shares the parse module's adjacency
        // contract with span building and doc collection.
        line: doc_run_start_line(node, source).unwrap_or_else(|| node.start_position().row + 1),
        docs,
        non_private,
        exception_scan,
        param_scan,
    });
}

/// The `<exception>` tags in `docs`: their count and every `cref` value.
fn exception_tags(docs: &[String]) -> (usize, Vec<String>) {
    let mut count = 0;
    let mut crefs = Vec::new();
    for tag in tag_slices(docs, "exception") {
        count += 1;
        if let Some(value) = attribute_value(tag, "cref") {
            crefs.push(value);
        }
    }
    (count, crefs)
}

/// The `name` attribute values of every `<param>` tag in `docs`.
fn param_tag_names(docs: &[String]) -> Vec<String> {
    tag_slices(docs, "param")
        .filter_map(|tag| attribute_value(tag, "name"))
        .collect()
}

/// True when `node`'s subtree holds a `throw_statement`.
///
/// One reused cursor walks the subtree depth-first; a fresh cursor per node
/// would allocate behind every step.
fn throws(node: tree_sitter::Node<'_>) -> bool {
    if node.kind() == "throw_statement" {
        return true;
    }
    let mut cursor = node.walk();
    'walk: loop {
        if cursor.goto_first_child() {
            if cursor.node().kind() == "throw_statement" {
                return true;
            }
            continue 'walk;
        }
        loop {
            if cursor.goto_next_sibling() {
                if cursor.node().kind() == "throw_statement" {
                    return true;
                }
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == node {
                return false;
            }
        }
    }
}

/// The quoted value of `attribute` inside a `<tag ...>` opening-tag slice.
fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let needle = format!("{attribute}=");
    let pos = tag.find(&needle)?;
    let rest = tag[pos + needle.len()..].trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &rest[1..];
    let end = value.find(quote)?;
    Some(value[..end].to_string())
}

/// The opening-tag text of every `<name ...>` tag in `docs`, scanned one
/// `///` line at a time; a tag split across lines does not match.
///
/// Tag names match whole: `<paramref ...>` is not a `<param>` tag, and
/// likewise for any longer tag sharing the sought prefix.
fn tag_slices<'a>(docs: &'a [String], tag: &str) -> impl Iterator<Item = &'a str> {
    docs.iter().flat_map(move |line| {
        let needle = format!("<{tag}");
        let mut rest = line.as_str();
        let mut out = Vec::new();
        while let Some(pos) = rest.find(&needle) {
            let after = &rest[pos..];
            // The byte after the tag name must end it: whitespace, `>`,
            // or the self-closing `/` of `<param/>`.
            let boundary = after[needle.len()..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/');
            if boundary {
                let end = after.find('>').map_or(rest.len(), |gt| pos + gt);
                out.push(&rest[pos..end]);
            }
            rest = &rest[pos + needle.len()..];
        }
        out.into_iter()
    })
}

#[cfg(test)]
mod tests {
    use super::tag_slices;

    // ── whole-tag-name matching ──

    /// `<paramref>` shares the `<param` prefix but is its own tag, so a
    /// `param` scan must not count it.
    #[test]
    fn tag_slices_skips_tags_sharing_only_a_prefix() {
        let docs = vec![" <paramref name=\"key\"/>".to_string()];

        assert_eq!(tag_slices(&docs, "param").count(), 0);
    }

    /// A real `<param>` tag matches exactly once, whether it carries
    /// attributes, closes, or self-closes.
    #[test]
    fn tag_slices_matches_whole_tag_names() {
        let docs = vec![
            " <param name=\"key\">The key.</param>".to_string(),
            " <param>".to_string(),
            " <param/>".to_string(),
        ];

        let slices: Vec<&str> = tag_slices(&docs, "param").collect();
        assert_eq!(slices.len(), 3, "one slice per whole-name tag: {slices:?}");
        assert!(slices[0].starts_with("<param name=\"key\""));
    }
}
