//! C# declaration facts shared by documentation rules and throw analysis.

use crate::languages::csharp::parse::{
    declaration_name, declared_return_type, doc_comment_texts, doc_run_start_line, member_kind,
    parameter_names, visibility_of,
};
use crate::source::{ItemKind, ReturnKind, VisibilityTier};

/// Kinds checked for parameter documentation (DOC004/DOC005); properties
/// cover indexers, whose parameter lists hold real parameters.
const PARAMETERIZED: &[ItemKind] = &[ItemKind::Fn, ItemKind::Constructor, ItemKind::Property];
/// Kinds checked for throwing, directly or through resolved calls
/// (DOC002/DOC003).
pub(crate) const THROWING: &[ItemKind] = &[ItemKind::Fn, ItemKind::Constructor];

/// One declaration's lint context: the shared facts every rule reads,
/// built once per declaration by the walker.
pub(crate) struct Declaration<'a> {
    /// The declaration's syntax node, for rules that walk attributes.
    pub(crate) node: tree_sitter::Node<'a>,
    /// The full source text, slicing companion to `node`.
    pub(crate) source: &'a str,
    /// The declaration's model kind.
    pub(crate) kind: ItemKind,
    /// The declaration's name, when it has a meaningful one.
    pub(crate) name: Option<String>,
    /// The innermost containing type, without namespace qualification.
    pub(crate) type_name: Option<String>,
    /// The declaration's `///` doc-comment lines.
    pub(crate) docs: Vec<String>,
    /// True when the visibility modifier is not `private`.
    pub(crate) non_private: bool,
    /// The 1-based diagnostic line: the `///` doc run's start when
    /// present, else the declaration's own row.
    pub(crate) line: usize,
    /// The `<exception>` tag facts for a non-private member that can
    /// throw, directly or through resolved calls: tag count plus every
    /// `cref` value.
    ///
    /// `None` for members that cannot or do not throw, so DOC002 and
    /// DOC003 share one answer; the lint dispatcher stamps it after the can-throw
    /// closure.
    pub(crate) exception_scan: Option<(usize, Vec<String>)>,
    /// The declared parameter names paired with their `<param>` tag names,
    /// for a non-private parameterized member that declares parameters.
    ///
    /// `None` otherwise, so DOC004 and DOC005 share one parameter walk.
    pub(crate) param_scan: Option<(Vec<String>, Vec<String>)>,
    /// For a non-private method: the [`ReturnKind`] of its declared
    /// return type, and whether its docs already carry a `<returns>`
    /// tag.
    ///
    /// DOC011 reads both from this one answer instead of rescanning
    /// the docs.
    ///
    /// `None` for private members and anything that is not a method.
    /// Unmodified bodyless interface methods count as non-private
    /// here: implicitly public in C#, unlike unmodified default
    /// implementations (with bodies), which stay private.
    pub(crate) returns: Option<(ReturnKind, bool)>,
}

/// Collect the facts of every declaration under `list` in document order.
///
/// `list` is a `compilation_unit` or `declaration_list`; collection
/// recurses into nested bodies and preprocessor branches.
/// `in_interface` reports whether `list` is an interface body, which
/// the `<returns>` visibility fact reads.
pub(crate) fn collect_children<'a>(
    list: tree_sitter::Node<'a>,
    source: &'a str,
    type_name: Option<&str>,
    in_interface: bool,
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
            let member = member_kind(kind);
            let nested_type = matches!(
                member,
                ItemKind::Class
                    | ItemKind::Struct
                    | ItemKind::Interface
                    | ItemKind::Record
                    | ItemKind::Enum
            );
            collect_declaration(child, source, type_name, in_interface, declarations);
            let nested_name = nested_type
                .then(|| declaration_name(child, source))
                .flatten();
            collect_children(
                body,
                source,
                nested_name.as_deref().or(type_name),
                nested_type && member == ItemKind::Interface,
                declarations,
            );
        } else if kind == "preproc_if" || kind == "preproc_else" || kind == "preproc_elif" {
            // Conditional branches hold real declarations; collect them.
            collect_children(child, source, type_name, in_interface, declarations);
        } else {
            collect_declaration(child, source, type_name, in_interface, declarations);
        }
    }
}

/// The `<exception>` tags in `docs`: their count and every `cref` value.
pub(crate) fn exception_tags(docs: &[String]) -> (usize, Vec<String>) {
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

/// The opening-tag text of every `<name ...>` tag in `docs`, scanned one
/// `///` line at a time; a tag split across lines does not match.
///
/// Tag names match whole: `<paramref ...>` is not a `<param>` tag, and
/// likewise for any longer tag sharing the sought prefix.
pub(crate) fn tag_slices<'a>(docs: &'a [String], tag: &str) -> impl Iterator<Item = &'a str> {
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

/// Collect one declaration node's facts into `declarations`.
///
/// Skips nodes that carry no member facts: usings, namespaces, and
/// unrecognized kinds.
fn collect_declaration<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a str,
    type_name: Option<&str>,
    in_interface: bool,
    declarations: &mut Vec<Declaration<'a>>,
) {
    let kind = member_kind(node.kind());
    if kind == ItemKind::Other || kind == ItemKind::Using || kind == ItemKind::Namespace {
        return;
    }

    // Shared facts, computed once per declaration rather than per rule.
    //
    // The <exception> facts are stamped in `run` after the can-throw
    // closure, which needs every declaration first.
    let non_private = visibility_of(node, source).is_some_and(|vis| vis != VisibilityTier::Private);
    let docs = doc_comment_texts(node, source);
    let param_scan = if non_private && PARAMETERIZED.contains(&kind) {
        let params = parameter_names(node, source);
        (!params.is_empty()).then(|| (params, param_tag_names(&docs)))
    } else {
        None
    };

    // DOC011 reads implicit interface publicity: an unmodified
    // bodyless method is public, while an unmodified default
    // implementation (a body) is interface-private.
    let returns_visible =
        non_private || (in_interface && implicitly_public_interface_method(node, source));
    let returns = (returns_visible && kind == ItemKind::Fn).then(|| {
        let kind = match declared_return_type(node, source) {
            Some("bool") => ReturnKind::Bool,
            Some("void") | None => ReturnKind::NoValue,
            Some(_) => ReturnKind::Value,
        };
        (kind, tag_slices(&docs, "returns").next().is_some())
    });
    declarations.push(Declaration {
        node,
        source,
        kind,
        name: declaration_name(node, source),
        type_name: type_name.map(str::to_owned),
        // The `///` doc run's line when present, else the declaration's
        // own row.
        //
        // doc_run_start_line shares the parse module's adjacency contract
        // with span building and doc collection.
        line: doc_run_start_line(node, source).unwrap_or_else(|| node.start_position().row + 1),
        docs,
        non_private,
        exception_scan: None,
        param_scan,
        returns,
    });
}

/// True when `node` is a bodyless method without a visibility
/// modifier inside an interface: implicitly public in C#, unlike an
/// unmodified default implementation, whose body keeps it private.
fn implicitly_public_interface_method(node: tree_sitter::Node<'_>, source: &str) -> bool {
    node.child_by_field_name("body").is_none() && {
        // Any visibility modifier (`public`, `private`, the
        // `protected` family, `internal`) restores modifier-based
        // visibility.
        let mut cursor = node.walk();
        !node.children(&mut cursor).any(|child| {
            child.kind() == "modifier"
                && matches!(
                    child.utf8_text(source.as_bytes()).unwrap_or(""),
                    "public" | "private" | "protected" | "internal"
                )
        })
    }
}

/// The `name` attribute values of every `<param>` tag in `docs`.
fn param_tag_names(docs: &[String]) -> Vec<String> {
    tag_slices(docs, "param")
        .filter_map(|tag| attribute_value(tag, "name"))
        .collect()
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
