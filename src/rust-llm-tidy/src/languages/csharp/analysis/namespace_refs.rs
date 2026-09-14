//! C# namespace reference facts over shared parses.
//!
//! [`NamespaceRefIndex`] maps every declared namespace to its
//! top-level type names. It also records which namespace's code
//! references which namespace's types, anchored to the referencing
//! file and line.
//!
//! Plain `using` directives open namespaces for bare-name resolution
//! without recording anything; `using static` directives record one
//! reference edge.
//!
//! The index drops references inside members marked `[Test]`,
//! `[TestMethod]`, `[Fact]`, or `[Theory]`.

use crate::languages::csharp::parse::has_test_marker;
use crate::source::ParseResult;
use ahash::AHashMap;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Member declarations whose attribute lists can gate off references.
const GATED_KINDS: &[&str] = &[
    "method_declaration",
    "constructor_declaration",
    "destructor_declaration",
    "operator_declaration",
    "conversion_operator_declaration",
    "property_declaration",
    "indexer_declaration",
    "event_declaration",
    "field_declaration",
    "event_field_declaration",
];
/// Type declaration kinds that name top-level namespace members.
const TYPE_KINDS: &[&str] = &[
    "class_declaration",
    "struct_declaration",
    "interface_declaration",
    "record_declaration",
    "record_struct_declaration",
    "enum_declaration",
    "delegate_declaration",
];

/// Declared C# namespaces, their top-level types, and reference edges.
#[derive(Clone, Default)]
pub struct NamespaceRefIndex {
    /// Each declared namespace's top-level type names.
    types: AHashMap<Box<str>, Vec<Box<str>>>,
    /// Reference edges in parse order, then source order.
    edges: Vec<NamespaceEdge>,
}

/// One namespace-to-namespace reference edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespaceEdge {
    /// The namespace containing the reference.
    pub from: Box<str>,
    /// The namespace of the referenced top-level type.
    pub target: Box<str>,
    /// The file anchoring the reference.
    pub file: PathBuf,
    /// The 1-based line of the reference.
    pub line: usize,
}

impl NamespaceRefIndex {
    /// Build namespace facts from the already-parsed project closure.
    ///
    /// # Arguments
    ///
    /// - `parses` - each C# parse result paired with its file path;
    ///   this constructor skips parses with syntax errors.
    ///
    /// # Returns
    ///
    /// The shared namespace map and reference edges across the
    /// supplied files, independent of the iteration order of `parses`.
    pub fn from_parses<'a>(parses: impl IntoIterator<Item = (&'a Path, &'a ParseResult)>) -> Self {
        let mut index = Self::default();
        let mut files = Vec::new();
        for (path, parsed) in parses {
            if parsed.syntax_tree().root_node().has_error() {
                continue;
            }
            let source = parsed.source.as_str();
            let root = parsed.syntax_tree().root_node();
            let mut namespaces = Vec::new();
            index.collect_declarations(root, "", source.as_bytes(), &mut namespaces);

            // Usings apply file-wide for bare-name resolution, so this
            // loop collects them before resolving any reference.
            let usings = collect_usings(root, source.as_bytes());
            files.push((path, parsed, namespaces, usings));
        }

        // Pass two walks only after every file's declarations are
        // known, so references resolve whichever order files arrive in.
        for (path, parsed, namespaces, usings) in &files {
            let first = namespaces.first().map(String::as_str).unwrap_or("");
            let root = parsed.syntax_tree().root_node();
            index.walk(root, path, "", first, usings, parsed.source.as_str());
        }
        index
    }

    /// The recorded reference edges.
    ///
    /// # Returns
    ///
    /// The edges in parse order, then source order.
    pub fn edges(&self) -> &[NamespaceEdge] {
        &self.edges
    }

    /// The top-level type names declared in one namespace.
    ///
    /// Test-facing declaration facts; the MOD004 analysis consumes
    /// edges only, so no production caller needs the raw map.
    ///
    /// # Arguments
    ///
    /// - `namespace` - the dotted namespace to look up.
    ///
    /// # Returns
    ///
    /// The declared type names, empty when the namespace declares no
    /// top-level types, or `None` when no indexed parse declares it.
    #[cfg(test)]
    pub fn declared_types(&self, namespace: &str) -> Option<&[Box<str>]> {
        self.types.get(namespace).map(Vec::as_slice)
    }

    /// Record each namespace's top-level types and namespace order.
    fn collect_declarations(
        &mut self,
        node: Node<'_>,
        ns: &str,
        bytes: &[u8],
        namespaces: &mut Vec<String>,
    ) {
        let mut cursor = node.walk();
        // A file-scoped namespace covers every following sibling, so
        // later members update `current` instead of recursing.
        let mut current = ns.to_string();
        for child in node.named_children(&mut cursor) {
            let kind = child.kind();
            if kind == "file_scoped_namespace_declaration"
                && let Some(qualified) = child_namespace(child, &current, bytes)
            {
                namespaces.push(qualified.clone());
                // Every declared namespace maps, even with no top-level
                // types of its own.
                self.types.entry(Box::from(qualified.as_str())).or_default();
                current = qualified;
            } else if kind == "namespace_declaration"
                && let Some(qualified) = child_namespace(child, &current, bytes)
            {
                namespaces.push(qualified.clone());
                self.types.entry(Box::from(qualified.as_str())).or_default();
                self.collect_declarations(child, &qualified, bytes, namespaces);
            } else if kind.starts_with("preproc_") || kind == "declaration_list" {
                self.collect_declarations(child, &current, bytes, namespaces);
            } else if !current.is_empty()
                && TYPE_KINDS.contains(&kind)
                && let Some(name) = child
                    .child_by_field_name("name")
                    .and_then(|name| leaf_name(name, bytes))
            {
                let names = self.types.entry(Box::from(current.as_str())).or_default();
                if !names.iter().any(|declared| declared.as_ref() == name) {
                    names.push(Box::from(name));
                }
            }
        }
    }

    /// Record references under `node`, attributing them to `ns`.
    fn walk(
        &mut self,
        node: Node<'_>,
        path: &Path,
        ns: &str,
        first_ns: &str,
        usings: &[String],
        source: &str,
    ) {
        let bytes = source.as_bytes();
        let mut cursor = node.walk();
        // A file-scoped namespace covers every following sibling.
        let mut current = ns.to_string();
        for child in node.named_children(&mut cursor) {
            let kind = child.kind();
            if matches!(kind, "comment" | "attribute_list" | "global_attribute") {
                continue;
            }
            if kind == "file_scoped_namespace_declaration"
                && let Some(qualified) = child_namespace(child, &current, bytes)
            {
                current = qualified;
            } else if kind == "namespace_declaration"
                && let Some(qualified) = child_namespace(child, &current, bytes)
            {
                self.walk(child, path, &qualified, first_ns, usings, source);
            } else if kind == "using_directive" {
                self.record_using(child, path, &current, first_ns, bytes);
            } else if kind.starts_with("preproc_") {
                self.walk(child, path, &current, first_ns, usings, source);
            } else if is_own_name(child) {
                continue;
            } else {
                if GATED_KINDS.contains(&kind) && has_test_marker(child, source) {
                    continue;
                }
                if is_occurrence_head(child)
                    && let Some(segments) = chain_segments(child, bytes)
                {
                    self.record_edge(&segments, child, path, &current, usings);
                }
                self.walk(child, path, &current, first_ns, usings, source);
            }
        }
    }

    /// Record one `using static` directive's resolved target. A
    /// directive outside any namespace block attributes to the file's
    /// first declared namespace.
    fn record_using(
        &mut self,
        node: Node<'_>,
        path: &Path,
        ns: &str,
        first_ns: &str,
        bytes: &[u8],
    ) {
        if !is_static_using(node) {
            return;
        }
        let from = if ns.is_empty() { first_ns } else { ns };
        if from.is_empty() {
            return;
        }
        if let Some(segments) = using_target(node, bytes)
            && let Some(target) = self.resolve_qualified(&segments)
            && target.as_ref() != from
        {
            self.edges.push(NamespaceEdge {
                from: Box::from(from),
                target,
                file: path.to_path_buf(),
                line: node.start_position().row + 1,
            });
        }
    }

    /// Record one reference occurrence when it resolves to a namespace.
    fn record_edge(
        &mut self,
        segments: &[&str],
        node: Node<'_>,
        path: &Path,
        ns: &str,
        usings: &[String],
    ) {
        if ns.is_empty() {
            return;
        }
        if let Some(target) = self.resolve(segments, ns, usings)
            && target.as_ref() != ns
        {
            self.edges.push(NamespaceEdge {
                from: Box::from(ns),
                target,
                file: path.to_path_buf(),
                line: node.start_position().row + 1,
            });
        }
    }

    /// Resolve one occurrence's segments to a declared namespace.
    ///
    /// Multi-segment chains try qualified resolution first, then fall
    /// back to the first segment as a bare name.
    fn resolve(&self, segments: &[&str], enclosing: &str, usings: &[String]) -> Option<Box<str>> {
        if segments.len() > 1
            && let Some(target) = self.resolve_qualified(segments)
        {
            return Some(target);
        }
        self.resolve_bare(segments[0], enclosing, usings)
    }

    /// Resolve a qualified chain: the longest namespace prefix whose
    /// following segment names a declared top-level type wins.
    fn resolve_qualified(&self, segments: &[&str]) -> Option<Box<str>> {
        for k in (1..segments.len()).rev() {
            let namespace = segments[..k].join(".");
            if self.declares(&namespace, segments[k]) {
                return Some(namespace.into_boxed_str());
            }
        }
        None
    }

    /// Resolve a bare name: the enclosing namespace first, then the
    /// longest matching `using` namespace.
    fn resolve_bare(&self, name: &str, enclosing: &str, usings: &[String]) -> Option<Box<str>> {
        if self.declares(enclosing, name) {
            return Some(Box::from(enclosing));
        }
        usings
            .iter()
            .filter(|ns| self.declares(ns, name))
            .max_by_key(|ns| ns.len())
            .map(|ns| ns.clone().into_boxed_str())
    }

    /// True when `namespace` declares a top-level type named `name`.
    fn declares(&self, namespace: &str, name: &str) -> bool {
        self.types
            .get(namespace)
            .is_some_and(|names| names.iter().any(|declared| declared.as_ref() == name))
    }
}

/// The qualified namespace of one namespace-declaration child within
/// `ns`, or `None` when it has no readable dotted name.
fn child_namespace(node: Node<'_>, ns: &str, bytes: &[u8]) -> Option<String> {
    let name = node
        .child_by_field_name("name")
        .and_then(|name| dotted_name(name, bytes))?;
    Some(if ns.is_empty() {
        name
    } else {
        format!("{ns}.{name}")
    })
}

/// Collect every plain `using` directive's namespace path in the file.
fn collect_usings(node: Node<'_>, bytes: &[u8]) -> Vec<String> {
    let mut usings = Vec::new();
    collect_using_directives(node, bytes, &mut usings);
    usings
}

/// True when `node` is the outermost link of a dotted name chain or a
/// bare identifier name, so it carries the whole reference.
fn is_occurrence_head(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let in_chain = matches!(parent.kind(), "qualified_name" | "member_access_expression");
    match node.kind() {
        "qualified_name" | "member_access_expression" => !in_chain,
        "identifier" => !in_chain && parent.kind() != "generic_name",
        "generic_name" => !in_chain,
        _ => false,
    }
}

/// True when `node` is the `name` field of its parent, so it is a
/// declaration's own name rather than a reference.
fn is_own_name(node: Node<'_>) -> bool {
    node.parent()
        .and_then(|parent| parent.child_by_field_name("name"))
        .is_some_and(|name| name.id() == node.id())
}

/// Add each plain, non-aliased `using` namespace under `node`.
fn collect_using_directives(node: Node<'_>, bytes: &[u8], usings: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "using_directive" {
            if !is_static_using(child)
                && child.child_by_field_name("name").is_none()
                && let Some(segments) = using_target(child, bytes)
            {
                usings.push(segments.join("."));
            }
        } else {
            collect_using_directives(child, bytes, usings);
        }
    }
}

/// The dot-joined dotted name of a namespace declaration's name node.
fn dotted_name(node: Node<'_>, bytes: &[u8]) -> Option<String> {
    Some(chain_segments(node, bytes)?.join("."))
}

/// True when `node` is a `using static` directive: its anonymous
/// children carry the `static` modifier token.
///
/// Mirrors the reader in mod003 `usings.rs`; shared hoisting lands
/// with the MOD004 wiring.
fn is_static_using(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| !child.is_named() && child.kind() == "static")
}

/// The target segments of one `using` directive, alias excluded.
/// Mirrors mod003 `usings.rs`' target reader; shared hoisting lands
/// with the MOD004 wiring.
fn using_target<'a>(node: Node<'_>, bytes: &'a [u8]) -> Option<Vec<&'a str>> {
    let alias = node.child_by_field_name("name");
    let alias_id = alias.map(|alias| alias.id());
    for i in 0..node.named_child_count() as u32 {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        if Some(child.id()) == alias_id
            || !matches!(
                child.kind(),
                "identifier" | "generic_name" | "qualified_name" | "alias_qualified_name"
            )
        {
            continue;
        }
        return chain_segments(child, bytes);
    }
    None
}

/// The dot-separated segments of a name chain, root first.
///
/// Covers both chain shapes (`qualified_name` in type positions,
/// `member_access_expression` in expression positions), bare and
/// generic names, and drops a `global::` alias prefix.
fn chain_segments<'a>(node: Node<'_>, bytes: &'a [u8]) -> Option<Vec<&'a str>> {
    let mut segments = Vec::new();
    let mut current = node;
    loop {
        match current.kind() {
            "qualified_name" => {
                segments.push(leaf_name(current.child_by_field_name("name")?, bytes)?);
                current = current.child_by_field_name("qualifier")?;
            }
            "member_access_expression" => {
                segments.push(leaf_name(current.child_by_field_name("name")?, bytes)?);
                current = current.child_by_field_name("expression")?;
            }
            "alias_qualified_name" => {
                current = current.child_by_field_name("name")?;
            }
            "identifier" | "generic_name" => {
                segments.push(leaf_name(current, bytes)?);
                break;
            }
            _ => return None,
        }
    }
    segments.reverse();
    Some(segments)
}

/// The name text of a name leaf: an identifier, or a `generic_name`
/// without its type arguments.
fn leaf_name<'a>(node: Node<'_>, bytes: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(bytes).ok(),
        "generic_name" => (0..node.named_child_count() as u32)
            .filter_map(|i| node.named_child(i))
            .find(|child| child.kind() == "identifier")
            .and_then(|identifier| identifier.utf8_text(bytes).ok()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{NamespaceEdge, NamespaceRefIndex};
    use crate::languages::csharp::parse::parse;
    use std::path::Path;

    /// Build an index over inline `(path, source)` fixtures.
    fn index_from(files: &[(&str, &str)]) -> NamespaceRefIndex {
        let parses: Vec<_> = files
            .iter()
            .map(|(path, source)| (*path, parse(source).expect("fixture parses")))
            .collect();
        NamespaceRefIndex::from_parses(
            parses
                .iter()
                .map(|(path, parsed)| (Path::new(path), parsed)),
        )
    }

    /// The declaring fixture most reference tests resolve against.
    const CORE_WIDGET: &str = "namespace App.Core\n{\n    class Widget { }\n}\n";

    /// A `run.cs` fixture referencing `Widget` from `App.Run` through
    /// `using App.Core;`: as a bare `Runner` field, or inside a
    /// `Check` method gated by `attribute`.
    fn widget_runner(attribute: Option<&str>) -> String {
        match attribute {
            None => "namespace App.Run\n{\n    using App.Core;\n\n    class Runner\n    {\n        Widget value;\n    }\n}\n".into(),
            Some(attribute) => format!(
                "namespace App.Run\n{{\n    using App.Core;\n\n    class Runner\n    {{\n        [{attribute}]\n        void Check()\n        {{\n            Widget value;\n        }}\n    }}\n}}\n"
            ),
        }
    }

    /// One expected edge for assertions.
    fn edge(from: &str, target: &str, file: &str, line: usize) -> NamespaceEdge {
        NamespaceEdge {
            from: from.into(),
            target: target.into(),
            file: file.into(),
            line,
        }
    }

    /// Declaration facts feed resolution for the reference tests.
    #[test]
    fn index_should_declare_top_level_types_when_nested_types_exist() {
        let index = index_from(&[(
            "lib.cs",
            r#"namespace Lib
{
    class Outer
    {
        class Inner { }
    }
}
"#,
        )]);

        let types = index.declared_types("Lib").map(|names| names.to_vec());
        assert_eq!(types, Some(vec!["Outer".into()]));
        assert_eq!(index.declared_types("Lib.Outer"), None);
        // A declared namespace with no top-level types still maps.
        let index = index_from(&[("empty.cs", "namespace Empty\n{\n}\n")]);
        assert_eq!(index.declared_types("Empty"), Some(&[][..]));
    }

    // Core behavior: resolution through usings and qualification.

    /// A bare sibling-namespace type name resolves through `using`.
    #[test]
    fn index_should_record_edge_when_bare_name_resolves_through_using() {
        let run = widget_runner(None);
        let index = index_from(&[("lib.cs", CORE_WIDGET), ("run.cs", &run)]);

        assert_eq!(index.edges(), [edge("App.Run", "App.Core", "run.cs", 7)]);
    }

    /// The same reference fully qualified resolves without any `using`.
    #[test]
    fn index_should_record_edge_when_reference_is_fully_qualified() {
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            (
                "run.cs",
                r#"namespace App.Run
{
    class Runner
    {
        App.Core.Widget value;
    }
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "App.Core", "run.cs", 5)]);
    }

    /// `using static` counts as one reference to the target's namespace.
    #[test]
    fn index_should_record_edge_when_using_static_names_declared_type() {
        let index = index_from(&[
            ("lib.cs", "namespace Lib\n{\n    class Maths { }\n}\n"),
            (
                "run.cs",
                r#"namespace App.Run
{
    using static Lib.Maths;
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "Lib", "run.cs", 3)]);
    }

    // Edge cases: silence and gating.

    /// A plain `using` alone opens a namespace but records nothing.
    #[test]
    fn index_should_stay_silent_when_using_directive_has_no_reference() {
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            ("run.cs", "namespace App.Run\n{\n    using App.Core;\n}\n"),
        ]);

        assert!(index.edges().is_empty());
    }

    /// Test-framework members are not production callers.
    #[test]
    fn index_should_drop_reference_when_inside_fact_method() {
        let run = widget_runner(Some("Fact"));
        let index = index_from(&[("lib.cs", CORE_WIDGET), ("run.cs", &run)]);

        assert!(index.edges().is_empty());
    }

    /// Qualified attribute spellings gate the same as bare ones.
    #[test]
    fn index_should_drop_reference_when_attribute_is_qualified() {
        let run = widget_runner(Some("Xunit.Fact"));
        let index = index_from(&[("lib.cs", CORE_WIDGET), ("run.cs", &run)]);

        assert!(index.edges().is_empty());
    }

    /// Framework and other unmatched names bind nothing.
    #[test]
    fn index_should_stay_silent_when_name_matches_no_declared_type() {
        let index = index_from(&[(
            "run.cs",
            r#"namespace App.Run
{
    using App.Core;

    class Runner
    {
        void Run()
        {
            Console.WriteLine("x");
        }
    }
}
"#,
        )]);

        assert!(index.edges().is_empty());
    }

    /// A reference to the enclosing namespace adds no self-edge.
    #[test]
    fn index_should_stay_silent_when_reference_targets_own_namespace() {
        let index = index_from(&[(
            "run.cs",
            r#"namespace App.Run
{
    class Widget { }

    class Runner
    {
        Widget value;
    }
}
"#,
        )]);

        assert!(index.edges().is_empty());
    }

    // Edge cases: parse shapes and order.

    /// Generic instantiations count like any other reference.
    #[test]
    fn index_should_record_edge_when_reference_is_generic_instantiation() {
        let index = index_from(&[
            ("lib.cs", "namespace App.Core\n{\n    class Box<T> { }\n}\n"),
            (
                "run.cs",
                r#"namespace App.Run
{
    using App.Core;

    class Runner
    {
        Box<Widget> value;
    }
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "App.Core", "run.cs", 7)]);
    }

    /// File-scoped namespaces resolve like block-scoped ones.
    #[test]
    fn index_should_record_edge_when_namespace_is_file_scoped() {
        let index = index_from(&[
            ("lib.cs", "namespace App.Core;\n\nclass Widget { }\n"),
            (
                "run.cs",
                r#"using App.Core;

namespace App.Run;

class Runner
{
    Widget value;
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "App.Core", "run.cs", 7)]);
    }

    /// A `global::` alias prefix still resolves the real namespace.
    #[test]
    fn index_should_record_edge_when_reference_uses_global_alias() {
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            (
                "run.cs",
                r#"namespace App.Run
{
    class Runner
    {
        global::App.Core.Widget value;
    }
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "App.Core", "run.cs", 5)]);
    }

    /// References resolve declarations from files iterated later.
    #[test]
    fn index_should_record_edge_when_declaring_file_comes_last() {
        let index = index_from(&[
            (
                "run.cs",
                r#"namespace App.Run
{
    using App.Core;

    class Runner
    {
        Widget value;
    }
}
"#,
            ),
            ("lib.cs", CORE_WIDGET),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "App.Core", "run.cs", 7)]);
    }

    /// The longest matching `using` namespace wins among candidates.
    #[test]
    fn index_should_pick_longest_using_when_two_usings_declare_name() {
        let index = index_from(&[
            ("shallow.cs", "namespace A\n{\n    class Widget { }\n}\n"),
            ("deep.cs", "namespace A.B\n{\n    class Widget { }\n}\n"),
            (
                "run.cs",
                r#"namespace App.Run
{
    using A;
    using A.B;

    class Runner
    {
        Widget value;
    }
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "A.B", "run.cs", 8)]);
    }

    /// Alias usings contribute nothing to bare-name resolution.
    #[test]
    fn index_should_stay_silent_when_using_is_alias() {
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            (
                "run.cs",
                r#"namespace App.Run
{
    using W = App.Core;

    class Runner
    {
        W value;
    }
}
"#,
            ),
        ]);

        assert!(index.edges().is_empty());
    }

    /// The constructor skips whole parses with syntax errors.
    #[test]
    fn index_should_stay_silent_when_parse_has_syntax_error() {
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            (
                "run.cs",
                "namespace App.Run\n{\n    class Runner\n    {\n        Widget value;\n",
            ),
        ]);

        assert!(index.edges().is_empty());
        assert_eq!(index.declared_types("App.Run"), None);
    }

    // Convenience: aggregation and attribution.

    /// References from several files aggregate under one `from` namespace.
    #[test]
    fn index_should_aggregate_edges_when_two_files_share_namespace() {
        let source = r#"namespace App.Run
{
    using App.Core;

    class Runner
    {
        Widget value;
    }
}
"#;
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            ("run1.cs", source),
            ("run2.cs", source),
        ]);

        assert_eq!(
            index.edges(),
            [
                edge("App.Run", "App.Core", "run1.cs", 7),
                edge("App.Run", "App.Core", "run2.cs", 7),
            ]
        );
    }

    /// A reference in a later namespace block attributes to that namespace.
    #[test]
    fn index_should_attribute_from_when_file_declares_second_namespace() {
        let index = index_from(&[
            ("lib.cs", CORE_WIDGET),
            (
                "run.cs",
                r#"namespace App.First
{
    using App.Core;

    class A
    {
        Widget value;
    }
}

namespace App.Second
{
    using App.Core;

    class B
    {
        Widget value;
    }
}
"#,
            ),
        ]);

        let froms: Vec<_> = index.edges().iter().map(|e| e.from.as_ref()).collect();
        assert_eq!(froms, ["App.First", "App.Second"]);
        assert!(
            index
                .edges()
                .iter()
                .all(|e| e.target.as_ref() == "App.Core")
        );
    }

    /// A nested-type reference edges to its top-level declaration's namespace.
    #[test]
    fn index_should_target_top_level_namespace_when_reference_reaches_nested_type() {
        let index = index_from(&[
            (
                "lib.cs",
                r#"namespace Lib
{
    class Outer
    {
        class Inner { }
    }
}
"#,
            ),
            (
                "run.cs",
                r#"namespace App.Run
{
    class Runner
    {
        Lib.Outer.Inner value;
    }
}
"#,
            ),
        ]);

        assert_eq!(index.edges(), [edge("App.Run", "Lib", "run.cs", 5)]);
    }
}
