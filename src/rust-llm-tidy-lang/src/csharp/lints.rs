//! C# lint checks: the XML doc-comment dialect over the same codes and
//! [`Diagnostic`] shape the Rust checks emit.
//!
//! [`run`] walks the compilation unit and every declaration list in
//! document order, one declaration at a time, emitting checks in the same
//! code order the Rust [`run_all`] uses (DOC001, DOC002, DOC003, DOC004,
//! DOC005, DOC006, TEST001).
//!
//! # Semantics
//!
//! - DOC001: non-private documentable declarations (`public`, `internal`,
//!   `protected`-family modifiers) need a `///` doc comment.
//! - DOC002: a non-private method or constructor whose body holds a
//!   `throw` needs an `<exception>` tag (warning severity; the body scan
//!   can miss rethrows through helpers).
//! - DOC003: throwing members whose `<exception>` tags all lack a
//!   concrete `cref` type.
//! - DOC004: non-private methods, constructors, and indexers with
//!   parameters need `<param name="...">` tags.
//! - DOC005: `<param>` tags must name every declared parameter.
//! - DOC006: placeholder markers (`TODO`/`FIXME`/`TBD`) in doc comments.
//! - TEST001: `TestMethod`/`Test`/`Fact`/`Theory`-marked methods with
//!   discouraged (`test_*`, `case_*`, `test` + digits) names.
//!
//! The parser-free text checks (DOC007/DOC008) never run for `.cs`; the
//! CLI admission registry keeps them markdown-family- and Rust-only.
//!
//! [`run_all`]: rust_llm_tidy_lint::check::run_all

use super::parse::{
    declaration_name, doc_comment_texts, has_test_marker, member_kind, parameter_names,
    visibility_of,
};
use rust_llm_tidy_lint::check::{
    CODE_DOC_PLACEHOLDER, CODE_MISSING_ARGUMENTS, CODE_MISSING_DOCS, CODE_MISSING_ERRORS,
    CODE_TEST_NAMING, CODE_UNDOCUMENTED_PARAM, CODE_VAGUE_ERRORS,
};
use rust_llm_tidy_lint::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, VisibilityTier};

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

/// Run every C# check over `parsed` and return all diagnostics in document
/// order.
///
/// Returns no diagnostics when the parse tree carries error nodes: a
/// broken tree would report findings against misread declarations, so the
/// whole pass degrades to silence.
pub(super) fn run(parsed: &ParseResult) -> Vec<Diagnostic> {
    if parsed.syntax_tree().root_node().has_error() {
        return Vec::new();
    }
    let source = parsed.source.as_str();
    let mut diagnostics = Vec::with_capacity(parsed.items.len());
    check_children(parsed.syntax_tree().root_node(), source, &mut diagnostics);
    diagnostics
}

/// Check every child of `list` (a `compilation_unit` or `declaration_list`)
/// and recurse into nested bodies and preprocessor branches.
fn check_children(list: tree_sitter::Node<'_>, source: &str, diagnostics: &mut Vec<Diagnostic>) {
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
            check_declaration(child, source, diagnostics);
            check_children(body, source, diagnostics);
        } else if kind == "preproc_if" || kind == "preproc_else" || kind == "preproc_elif" {
            // Conditional branches hold real declarations; check them.
            check_children(child, source, diagnostics);
        } else {
            check_declaration(child, source, diagnostics);
        }
    }
}

/// Run every check over one declaration node.
fn check_declaration(node: tree_sitter::Node<'_>, source: &str, diagnostics: &mut Vec<Diagnostic>) {
    let kind = member_kind(node.kind());
    if kind == ItemKind::Other || kind == ItemKind::Using || kind == ItemKind::Namespace {
        return;
    }
    let name = declaration_name(node, source);
    let docs = doc_comment_texts(node, source);
    let non_private = visibility_of(node, source).is_some_and(|vis| vis != VisibilityTier::Private);
    let line = doc_start_line(node, source).unwrap_or_else(|| node.start_position().row + 1);
    let item_kind = kind.as_str().to_string();
    let mut push = |severity: Severity, code: &'static str, message: String| {
        diagnostics.push(Diagnostic {
            severity,
            code,
            message,
            line,
            item_kind: item_kind.clone(),
            item_name: name.clone(),
        });
    };

    // DOC001: non-private documentable declarations need docs.
    if non_private && DOCUMENTABLE.contains(&kind) && docs.is_empty() {
        push(
            Severity::Error,
            CODE_MISSING_DOCS,
            "non-private item is missing a doc comment".to_string(),
        );
    }

    // DOC002/DOC003: throwing members need `<exception>` tags with a
    // concrete `cref`.
    if non_private && THROWING.contains(&kind) && throws(node) {
        let (tag_count, crefs) = exception_tags(&docs);
        if tag_count == 0 {
            push(
                Severity::Warning,
                CODE_MISSING_ERRORS,
                "member that throws is missing an `<exception>` doc tag".to_string(),
            );
        } else if !crefs.iter().any(|cref| !cref.trim().is_empty()) {
            push(
                Severity::Warning,
                CODE_VAGUE_ERRORS,
                "`<exception>` doc tags name no concrete exception type (`cref`)".to_string(),
            );
        }
    }

    // DOC004/DOC005: `<param>` tags against the declared parameters.
    if non_private && PARAMETERIZED.contains(&kind) {
        let params = parameter_names(node, source);
        if !params.is_empty() {
            let tags = param_tag_names(&docs);
            if tags.is_empty() {
                push(
                    Severity::Warning,
                    CODE_MISSING_ARGUMENTS,
                    "member with parameters is missing `<param>` doc tags".to_string(),
                );
            } else {
                let missing: Vec<&str> = params
                    .iter()
                    .map(String::as_str)
                    .filter(|p| !tags.iter().any(|tag| tag == p))
                    .collect();
                if !missing.is_empty() {
                    push(
                        Severity::Warning,
                        CODE_UNDOCUMENTED_PARAM,
                        format!(
                            "parameter(s) not documented in `<param>` tags: `{}`",
                            missing.join("`, `")
                        ),
                    );
                }
            }
        }
    }

    // DOC006: placeholder markers in doc comments.
    if DOCUMENTABLE.contains(&kind)
        && docs.iter().any(|doc| {
            ["todo", "fixme", "tbd"]
                .iter()
                .any(|m| contains_word(doc, m))
        })
    {
        push(
            Severity::Warning,
            CODE_DOC_PLACEHOLDER,
            "doc comment contains placeholder text (TODO/FIXME/TBD)".to_string(),
        );
    }

    // TEST001: marker-attributed methods need behavioral names.
    if has_test_marker(node, source)
        && let Some(name) = name.as_deref()
        && is_bad_test_name(name)
    {
        push(
            Severity::Warning,
            CODE_TEST_NAMING,
            format!(
                "test method `{name}` should use a behavioral name \
                 (subject_should_expectation_when_condition), not a `test_*` or `case_*` prefix"
            ),
        );
    }
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric, non-underscore character (or
/// the start/end of the text), mirroring the lint crate's Rust checks: the
/// marker matches in `TODO:` but not in `todolist`.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let lower = haystack.to_ascii_lowercase();
    let mut start = 0;
    while let Some(pos) = lower[start..].find(needle) {
        let abs = start + pos;
        let before_ok = lower[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let after = abs + needle.len();
        let after_ok = lower[after..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if before_ok && after_ok {
            return true;
        }
        start = abs + needle.len();
    }
    false
}

/// 1-based line of `node`'s `///` doc run, if it has one.
///
/// Delegates to the parse module's doc-run walk so span building, lint
/// line anchoring, and doc collection share one adjacency contract.
fn doc_start_line(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    super::parse::doc_run_start_line(node, source)
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

/// True when `name` uses a discouraged test-naming pattern.
///
/// ASCII case-insensitive counterpart of the Rust rule: the bare `test`
/// name, `test_*`/`case_*` prefixes, and `test` immediately followed by
/// digits; behavioral names like `ShouldReturnNullWhenMissing` pass.
fn is_bad_test_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "test" || lower.starts_with("test_") || lower.starts_with("case_") {
        return true;
    }
    lower
        .strip_prefix("test")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
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
fn tag_slices<'a>(docs: &'a [String], tag: &str) -> impl Iterator<Item = &'a str> {
    docs.iter().flat_map(move |line| {
        let needle = format!("<{tag}");
        let mut rest = line.as_str();
        let mut out = Vec::new();
        while let Some(pos) = rest.find(&needle) {
            let after = &rest[pos..];
            let end = after.find('>').map_or(rest.len(), |gt| pos + gt);
            out.push(&rest[pos..end]);
            rest = &rest[pos + needle.len()..];
        }
        out.into_iter()
    })
}
