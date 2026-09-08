//! `MOD003`: flag paths that include the full namespace.
//!
//! A qualified name spells out where a name lives, such as `System.Console`.
//! This rule reads the file's syntax and emits hints; it does not rewrite code
//! or ask the compiler to resolve names.
//!
//! # Explanation 1: reuse an existing import
//!
//! A plain `using` lets the replacement omit the namespace prefix.
//!
//! The walker tracks imports and names in nested scopes, then looks for the
//! longest import matching the beginning of a name.
//!
//! ```csharp
//! using System.Threading.Tasks;
//!
//! System.Threading.Tasks.Task.Delay(1);
//! Task.Delay(1);
//! ```
//!
//! Here, `using System.Threading.Tasks;` makes `Task` available without its
//! namespace. An alias supplies a replacement name: `using Log = System.Console;`
//! allows `Log.WriteLine("before")`.
//!
//! # Explanation 2: suggest a missing import
//!
//! Without a matching import, the rule suggests a namespace `using` at namespace
//! or file scope, subject to checking that the prefix is a namespace.
//!
//! Type positions suggest importing the parent; expressions suggest importing
//! only the root, preserving possible type and property segments.
//!
//! ```csharp
//! // Before: no import is needed for the long spelling.
//! class C { System.Text.StringBuilder Create() => new(); }
//! ```
//!
//! ```csharp
//! // After: import the namespace, not the type.
//! using System.Text;
//!
//! class C { StringBuilder Create() => new(); }
//! ```
//!
//! Syntax does not distinguish namespaces from containing types. If the proposed
//! prefix is a type, use a type alias or retain qualification instead.
//!
//! `global::` explicitly roots a path. Unshadowed `System` and `Microsoft`
//! are known roots. Other unprefixed roots remain exempt: imports and type
//! positions do not prove that a name includes its full namespace.
//!
//! # Code walkthrough: start at `check`
//!
//! Read these functions in call order, not their order in the file.
//!
//! 1. [`check`] receives an already-parsed file. It creates a [`Walker`], visits
//!    the syntax tree, and returns the collected diagnostics.
//! 2. [`Walker::collect_relative_roots`] finds namespace components that could
//!    make a known root relative rather than absolute.
//! 3. [`Walker::walk`] visits syntax nodes recursively. Entering a scope pushes
//!    a [`ScopeFrame`]; leaving a nested scope pops it.
//! 4. [`is_chain_head`] selects the outermost node of a dotted chain. For
//!    `System.Console.WriteLine`, this avoids separate hints for shorter prefixes.
//!    [`Walker::record_occurrence`] then checks the name's eligibility.
//! 5. [`Walker::covering_import`] finds the longest visible import prefix.
//!    [`Walker::suggestion_under`] removes a plain import's prefix or replaces
//!    an aliased prefix with its alias.
//! 6. [`Walker::record_occurrence`] adds a hint only when it has advice.
//!    [`Walker::enclosing_item`] supplies the containing item's kind and name;
//!    the name node supplies the line number. [`check`] returns these hints.
//!
//! ## What the stored data means
//!
//! - [`Walker`]: source bytes, relative roots, active scopes, and accumulated hints
//! - [`ScopeFrame`]: imports and bound names in one active scope
//! - [`Import`]: an imported path, its name, and whether it is an alias
//! - [`Binding`]: a declared name and the position where it starts counting
//! - [`Suggestion`]: the import's name for the hint and the replacement text
//!
//! The scope stack models nested visibility, not compiler name resolution.
//!
//! [`frame_mentions`] asks whether a scope uses a name; [`frame_shadows`] asks
//! whether it conflicts with the proposed import.
//!
//! Imports and declaration names are collected before visiting a scope's children.
//! A binding's `start` value distinguishes scope-wide declarations from locals
//! that count only from their position.
//!
//! Without a covering import, `record_occurrence` builds the namespace advice
//! shown in Explanation 2. It also rejects roots bound to a local or declaration.
//!
//! ## Trace the first example
//!
//! `collect_usings` records `using System.Threading.Tasks;` as a plain import.
//!
//! `name_segments` splits the path into `System`, `Threading`, `Tasks`, `Task`,
//! and `Delay`.
//!
//! The covering import matches `System.Threading.Tasks`. `suggestion_under`
//! removes that prefix, leaving `Task.Delay` if the short name does not conflict.
//!
//! Next: open [`check`], then follow [`Walker::walk`] to
//! [`Walker::record_occurrence`] with this example in mind.
//!
//! # Remarks
//!
//! Hints are withheld for conflicting short names, unknown expression receivers,
//! conditional methods or regions, attributes, import text, and generic chains.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_QUALIFIED_PATH;
use crate::source::{ItemKind, ParseResult};
use std::collections::HashSet;
use tree_sitter::Node;

/// Known namespace roots, unless syntax shows shadowing or relative use.
const ROOT_SEGMENTS: &[&str] = &["System", "Microsoft"];

/// What an existing import advises: the covering directive's
/// name for the hint and the replacement text for the whole path.
///
/// An aliased directive (`using W = Nq.Text.Widget;`) binds `W`, so a
/// covered path suggests the alias plus the remaining segments
/// (`W.Empty`).
///
/// A plain `using` opens its namespace, so the covered prefix
/// disappears and the remaining segments read unqualified (`Empty`).
struct Suggestion<'a> {
    short: &'a str,
    replacement: String,
}

/// Lexical scopes and hints collected in source order.
struct Walker<'a> {
    bytes: &'a [u8],
    relative_roots: HashSet<&'a str>,
    scopes: Vec<ScopeFrame<'a>>,
    diagnostics: Vec<Diagnostic>,
}

/// The imports and names one lexical scope introduces during the walk.
#[derive(Default)]
struct ScopeFrame<'a> {
    imports: Vec<Import<'a>>,
    bindings: Vec<Binding<'a>>,
}

/// One name a lexical scope binds: a local designation (parameter,
/// variable, foreach or catch name) or a same-scope declaration.
///
/// `start == 0` marks a declaration: visible throughout its scope.
/// Any other `start` is the binding's byte offset; a local shadows
/// only from that position onward.
struct Binding<'a> {
    name: &'a str,
    start: usize,
}

/// One explicit `using` import: its full path, its bound name (an
/// alias, else the last segment), and whether an alias binds it.
///
/// A plain, non-aliased directive binds nothing itself: it opens its
/// namespace so the namespace's members read unqualified.
struct Import<'a> {
    segments: Vec<&'a str>,
    short: &'a str,
    aliased: bool,
    /// Whether the directive bypasses relative name resolution with `global::`.
    absolute: bool,
}

impl<'a> Walker<'a> {
    /// Withhold unprefixed roots declared below another namespace anywhere in the file.
    ///
    /// Separate namespace blocks can contribute members to the same namespace,
    /// so lexical scope alone cannot exclude relative resolution.
    fn collect_relative_roots(&mut self, node: Node) {
        if matches!(
            node.kind(),
            "namespace_declaration" | "file_scoped_namespace_declaration"
        ) && let Some(name) = node.child_by_field_name("name")
            && let Some(segments) = self.name_segments(name)
        {
            let top_level = node
                .parent()
                .is_some_and(|p| p.kind() == "compilation_unit");
            self.relative_roots.extend(
                segments
                    .into_iter()
                    .skip(usize::from(top_level))
                    .map(|s| s.trim_start_matches('@')),
            );
        }

        for i in 0..node.named_child_count() as u32 {
            if let Some(child) = node.named_child(i) {
                self.collect_relative_roots(child);
            }
        }
    }

    /// Walk `node`'s subtree: maintain scope frames, record
    /// qualified-path occurrences, and never descend into exempt spans.
    fn walk(&mut self, node: Node) {
        if node.kind().starts_with("preproc_if")
            || (matches!(
                node.kind(),
                "method_declaration"
                    | "constructor_declaration"
                    | "destructor_declaration"
                    | "operator_declaration"
                    | "conversion_operator_declaration"
                    | "local_function_statement"
            ) && contains_conditional(node))
        {
            return;
        }

        match node.kind() {
            // D7 exemptions: `using` text and attribute spans produce
            // no occurrences; a `using` contributes its imports to the
            // current scope instead.
            "using_directive" => return,

            "attribute_list" | "global_attribute" => return,
            _ => {}
        }

        let pushed = if is_scope(node.kind()) {
            self.scopes.push(ScopeFrame::default());
            // Declarations and usings are visible throughout their scope.
            for i in 0..node.named_child_count() as u32 {
                if let Some(child) = node.named_child(i) {
                    self.record_item_name(child);
                    if child.kind() == "using_directive" {
                        self.record_use(child);
                    }
                }
            }
            true
        } else {
            false
        };
        self.record_bindings(node);
        if is_chain_head(node) {
            self.record_occurrence(node);
        }

        for i in 0..node.named_child_count() as u32 {
            if let Some(child) = node.named_child(i) {
                self.walk(child);
            }
        }
        if pushed && self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Add a `using` directive's imports to the innermost scope frame.
    fn record_use(&mut self, node: Node) {
        let imports = self.collect_usings(node);
        let alias = node
            .child_by_field_name("name")
            .and_then(|n| self.leaf_text(n));

        if let Some(frame) = self.scopes.last_mut() {
            // Even unsupported alias targets bind their alias, never a namespace root.
            if imports.is_empty()
                && let Some(name) = alias
            {
                frame.bindings.push(Binding { name, start: 0 });
            }
            frame.imports.extend(imports);
        }
    }

    /// Record a declaration node's own name into the current scope
    /// frame.
    ///
    /// Declarations (namespaces, types, members, local functions)
    /// bind their name in the enclosing scope, order-independent.
    /// Callers run this before the node's own frame is pushed, so
    /// member names land around the member, not inside it.
    fn record_item_name(&mut self, node: Node) {
        let names: Vec<&'a str> = match node.kind() {
            "namespace_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| self.leftmost_segment(n))
                .into_iter()
                .collect(),
            "extern_alias_directive"
            | "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "record_declaration"
            | "record_struct_declaration"
            | "enum_declaration"
            | "enum_member_declaration"
            | "delegate_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "destructor_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration"
            | "property_declaration"
            | "indexer_declaration"
            | "event_declaration"
            | "local_function_statement" => node
                .child_by_field_name("name")
                .and_then(|n| self.leaf_text(n))
                .into_iter()
                .collect(),
            // Fields and event fields declare their variables in
            // declarator children; the names are visible throughout
            // the type body.
            "field_declaration" | "event_field_declaration" => self.declarator_names(node),
            _ => return,
        };
        if let Some(frame) = self.scopes.last_mut() {
            frame
                .bindings
                .extend(names.into_iter().map(|name| Binding { name, start: 0 }));
        }
    }

    /// Record the names `node` binds into the current scope frame.
    ///
    /// Local designations (declarations, parameters, out-arguments,
    /// foreach and catch names, pattern designations, query variables)
    /// bind from their position onward.
    ///
    /// A field's declarator lands here too, redundantly with its
    /// order-independent declaration binding.
    fn record_bindings(&mut self, node: Node) {
        let mut names: Vec<(&'a str, usize)> = Vec::new();
        match node.kind() {
            "parameter" | "catch_declaration" | "type_parameter" => {
                names.extend(
                    node.child_by_field_name("name")
                        .and_then(|n| self.leaf_text(n))
                        .map(|name| (name, node.start_byte())),
                );
            }
            // An out-argument (`out var t`) or a positional pattern's
            // sub-designation (`Pair(int a, int t)`).
            "declaration_expression" | "from_clause" => {
                names.extend(self.field_name_binding(node));
            }
            // A query's `into` continuation names its range variable:
            // the identifier sits between clauses without its own node.
            "query_expression" => {
                for i in 0..node.named_child_count() as u32 {
                    if let Some(child) = node.named_child(i)
                        && child.kind() == "identifier"
                        && let Some(name) = self.leaf_text(child)
                    {
                        names.push((name, child.start_byte()));
                    }
                }
            }
            // `let t = ...` carries its name as the leading identifier
            // child, ahead of the value expression.
            "let_clause" => {
                if let Some(child) = node.named_child(0)
                    && child.kind() == "identifier"
                    && let Some(name) = self.leaf_text(child)
                {
                    names.push((name, child.start_byte()));
                }
            }
            // A `join t in ys` clause's range variable is the first
            // identifier after the clause's optional type; later
            // identifiers are the source and references.
            "join_clause" => {
                let type_id = node.child_by_field_name("type").map(|ty| ty.id());
                for i in 0..node.named_child_count() as u32 {
                    let Some(child) = node.named_child(i) else {
                        continue;
                    };
                    if Some(child.id()) == type_id {
                        continue;
                    }
                    if child.kind() == "identifier"
                        && let Some(name) = self.leaf_text(child)
                    {
                        names.push((name, child.start_byte()));
                    }
                    break;
                }
            }
            // A `join ... into t` continuation names its range
            // variable.
            "join_into_clause" => {
                for i in 0..node.named_child_count() as u32 {
                    if let Some(child) = node.named_child(i)
                        && child.kind() == "identifier"
                        && let Some(name) = self.leaf_text(child)
                    {
                        names.push((name, child.start_byte()));
                    }
                }
            }
            // A parenthesized designation (`var (a, t)`) binds each
            // identifier it lists; nested designations recurse by the
            // walk.
            "parenthesized_variable_designation" => {
                for i in 0..node.named_child_count() as u32 {
                    if let Some(child) = node.named_child(i)
                        && child.kind() == "identifier"
                        && let Some(name) = self.leaf_text(child)
                    {
                        names.push((name, child.start_byte()));
                    }
                }
            }
            "variable_declarator" => {
                names.extend(
                    node.child_by_field_name("name")
                        .and_then(|n| self.leaf_text(n))
                        .map(|name| (name, node.start_byte())),
                );
                // A deconstruction declaration (`var (a, b) = ...`)
                // binds its pattern names instead of a declarator name.
                for i in 0..node.named_child_count() as u32 {
                    if let Some(child) = node.named_child(i)
                        && child.kind() == "tuple_pattern"
                    {
                        names.extend(
                            self.tuple_pattern_names(child)
                                .into_iter()
                                .map(|name| (name, node.start_byte())),
                        );
                    }
                }
            }
            // An `is` or `case` pattern designation (`o is int t`)
            // binds its name from the pattern onward.
            "declaration_pattern" => {
                names.extend(
                    node.child_by_field_name("name")
                        .and_then(|n| self.leaf_text(n))
                        .map(|name| (name, node.start_byte())),
                );
            }
            // A simple lambda's parameter is its own aliased identifier
            // node, so the text is the name.
            "implicit_parameter" => {
                names.extend(
                    node.utf8_text(self.bytes)
                        .ok()
                        .map(|name| (name, node.start_byte())),
                );
            }
            "foreach_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    names.extend(
                        self.designation_names(left)
                            .into_iter()
                            .map(|name| (name, node.start_byte())),
                    );
                }
            }
            _ => {}
        }
        if let Some(frame) = self.scopes.last_mut() {
            frame.bindings.extend(
                names
                    .into_iter()
                    .map(|(name, start)| Binding { name, start }),
            );
        }
    }

    /// The name `node` binds through its `name` field, positioned at
    /// the name itself.
    fn field_name_binding(&self, node: Node) -> Option<(&'a str, usize)> {
        let name = node.child_by_field_name("name")?;
        self.leaf_text(name).map(|text| (text, name.start_byte()))
    }

    /// Suggest a readable replacement for one eligible qualified name.
    fn record_occurrence(&mut self, node: Node) {
        if node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "namespace_declaration" | "file_scoped_namespace_declaration"
            )
        }) {
            return;
        }

        let Some(segments) = self.name_segments(node) else {
            return; // not a plain dotted chain: file-local text cannot decide
        };
        let absolute = self
            .leftmost_name(node)
            .is_some_and(|n| n.kind() == "alias_qualified_name");
        if segments.len() < 2 {
            return;
        }
        if !absolute
            && (!ROOT_SEGMENTS.contains(&segments[0])
                || self.relative_roots.contains(segments[0])
                || self.scopes.iter().any(|frame| {
                    frame.imports.iter().any(|import| {
                        import.aliased && import.short.trim_start_matches('@') == segments[0]
                    }) || frame.bindings.iter().any(|binding| {
                        binding.name.trim_start_matches('@') == segments[0]
                            && (binding.start == 0 || binding.start <= node.start_byte())
                    })
                }))
        {
            return;
        }

        let line = node.start_position().row + 1;
        let (kind, name) = self.enclosing_item(node);
        let covering = self.covering_import(&segments, absolute);
        let prefix = if absolute { "global::" } else { "" };
        let path = format!("{prefix}{}", segments.join("."));
        let advice = if let Some(matched) = covering {
            self.suggestion_under(matched, &segments, node.start_byte())
                .map(|suggestion| {
                    format!(
                        "- If clear at the use site, use `{}`; `{}` is already imported.",
                        suggestion.replacement, suggestion.short
                    )
                })
        } else {
            // Expressions retain everything below the root: syntax cannot
            // distinguish namespace, type, and static-property segments.
            let imported_len = if node.kind() == "member_access_expression" {
                2
            } else {
                segments.len()
            };
            let short = segments[imported_len - 1];
            let shadowed = self
                .scopes
                .iter()
                .any(|frame| frame_mentions(frame, short, node.start_byte()));
            (!shadowed).then(|| {
                let namespace = format!("{prefix}{}", segments[..imported_len - 1].join("."));
                let replacement = segments[imported_len - 1..].join(".");
                format!("- If `{namespace}` is a namespace and the result is clear, add `using {namespace};` at namespace or file scope and use `{replacement}`.")
            })
        };

        if let Some(advice) = advice {
            self.diagnostics.push(Diagnostic {
                severity: Severity::Hint,
                code: CODE_QUALIFIED_PATH,
                message: format!(
                    "path `{path}` includes the full namespace.\n\
                     - Shorten with imports or aliases only if the meaning remains clear at the use site.\n\
                     {advice}\n\
                     - Retain namespace or type context when needed; use a type alias if the proposed import targets a containing type, not a namespace.\n\
                     - Keep the full path if shortening would reduce clarity or create a name conflict."
                ),
                line,
                item_kind: kind.to_string(),
                item_name: name.map(str::to_string),
            });
        }
    }

    /// What to suggest for `segments` under the covering
    /// import `matched`.
    ///
    /// An alias binds its name, so the advice reads the alias plus the
    /// remaining segments. A plain `using` opens its namespace: the
    /// covered prefix disappears and the remaining segments read
    /// unqualified.
    ///
    /// A plain `using` covering the exact path has no advice: it
    /// imports the namespace's members, never the namespace's own
    /// simple name.
    ///
    /// Suppresses when the innermost frame mentioning the name the
    /// advice references shadows or ambiguates it. That name is the alias
    /// or the plain using's first remaining segment.
    ///
    /// Such mentions are a local designation, a declaration, or an
    /// import of a different path under the same name. A local
    /// positioned after the occurrence is not yet a mention.
    fn suggestion_under(
        &self,
        matched: &Import<'a>,
        segments: &[&str],
        occurrence_start: usize,
    ) -> Option<Suggestion<'a>> {
        let tail = &segments[matched.segments.len()..];
        let referenced = if matched.aliased {
            matched.short
        } else {
            tail.split_first()?.0
        };
        let shadowed = self
            .scopes
            .iter()
            .rev()
            .find(|frame| frame_mentions(frame, referenced, occurrence_start))
            .is_some_and(|frame| {
                frame_shadows(frame, referenced, &matched.segments, occurrence_start)
                    || frame.imports.iter().any(|import| {
                        import.short == referenced && import.absolute != matched.absolute
                    })
            });
        if shadowed {
            return None;
        }
        let replacement = if !matched.aliased {
            tail.join(".")
        } else if tail.is_empty() {
            matched.short.to_string()
        } else {
            format!("{}.{}", matched.short, tail.join("."))
        };
        Some(Suggestion {
            short: matched.short,
            replacement,
        })
    }

    /// The longest in-scope import covering `segments`: its path equals
    /// the occurrence path or prefixes it.
    ///
    /// The innermost frame wins ties, since its binding shadows outer
    /// ones. Absolute occurrences only reuse absolute imports, avoiding relative
    /// targets that happen to have the same spelling.
    ///
    /// Root namespace imports such as `using System;` also cover longer paths.
    fn covering_import(&self, segments: &[&str], absolute: bool) -> Option<&Import<'a>> {
        let mut covering: Option<&Import<'a>> = None;
        for frame in self.scopes.iter().rev() {
            for import in &frame.imports {
                let covers = segments.starts_with(import.segments.as_slice())
                    && (!absolute || import.absolute);
                if covers
                    && covering
                        .as_ref()
                        .is_none_or(|best| import.segments.len() > best.segments.len())
                {
                    covering = Some(import);
                }
            }
        }
        covering
    }

    /// The explicit imports of one `using` directive.
    ///
    /// A plain directive opens its namespace and binds nothing; its
    /// `short` (the last segment) still feeds the hint's name and the
    /// mention checks. An aliased directive (`using X = A.B.C;`)
    /// binds the alias.
    ///
    /// Static imports cannot supply named advice without type checking.
    fn collect_usings(&self, node: Node) -> Vec<Import<'a>> {
        let mut imports = Vec::new();
        if self.is_static_using(node) {
            return imports;
        }

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
            let Some(segments) = self.name_segments(child) else {
                break;
            };
            let bound = alias.and_then(|alias| self.leaf_text(alias));
            let short = bound.unwrap_or_else(|| *segments.last().expect("using path has a name"));
            imports.push(Import {
                segments,
                short,
                aliased: bound.is_some(),
                absolute: self
                    .leftmost_name(child)
                    .is_some_and(|n| n.kind() == "alias_qualified_name"),
            });
            break;
        }
        imports
    }

    /// True when `node` is a `using static` directive: its anonymous
    /// children carry the `static` modifier token.
    fn is_static_using(&self, node: Node) -> bool {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .any(|child| !child.is_named() && child.kind() == "static")
    }

    /// The leftmost segment of a dotted name chain, without building the
    /// whole segment list.
    fn leftmost_segment(&self, node: Node) -> Option<&'a str> {
        self.leaf_text(self.leftmost_name(node)?)
    }

    /// Locate the root syntax without mistaking an alias for an ordinary identifier.
    fn leftmost_name<'tree>(&self, node: Node<'tree>) -> Option<Node<'tree>> {
        let mut current = node;
        loop {
            match current.kind() {
                "qualified_name" => current = current.child_by_field_name("qualifier")?,
                "member_access_expression" => {
                    current = current.child_by_field_name("expression")?
                }
                "identifier" | "alias_qualified_name" => return Some(current),
                _ => return None,
            }
        }
    }

    /// The dot-joined segments of a qualified-name chain, root first.
    ///
    /// Handles both shapes a dotted path takes: `qualified_name` in
    /// type positions and `member_access_expression` in expression
    /// positions.
    ///
    /// Returns `None` when the chain does not bottom out in plain
    /// identifiers or `global::`. Generic chains are exempt rather than losing
    /// type arguments in advice; non-global aliases never establish full paths.
    fn name_segments(&self, node: Node) -> Option<Vec<&'a str>> {
        let mut segments = Vec::new();
        let mut current = node;
        loop {
            match current.kind() {
                "qualified_name" => {
                    let name = current.child_by_field_name("name")?;
                    if name.kind() != "identifier" {
                        return None;
                    }
                    segments.push(self.leaf_text(name)?);
                    current = current.child_by_field_name("qualifier")?;
                }
                "member_access_expression" => {
                    let name = current.child_by_field_name("name")?;
                    if name.kind() != "identifier" {
                        return None;
                    }
                    segments.push(self.leaf_text(name)?);
                    current = current.child_by_field_name("expression")?;
                }
                "alias_qualified_name" => {
                    if self.leaf_text(current.child_by_field_name("alias")?)? != "global" {
                        return None;
                    }
                    current = current.child_by_field_name("name")?;
                }
                "identifier" => {
                    segments.push(self.leaf_text(current)?);
                    break;
                }
                _ => return None,
            }
        }
        segments.reverse();
        Some(segments)
    }

    /// The variable names a field or event-field declaration declares.
    fn declarator_names(&self, node: Node) -> Vec<&'a str> {
        let mut names = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "variable_declaration" {
                continue;
            }
            let mut declarators = child.walk();
            for declarator in child.children(&mut declarators) {
                if declarator.kind() == "variable_declarator"
                    && let Some(name) = declarator
                        .child_by_field_name("name")
                        .and_then(|name| self.leaf_text(name))
                {
                    names.push(name);
                }
            }
        }
        names
    }

    /// The names a foreach designation binds: a simple name, a
    /// destructuring pattern, or a declaration's variable.
    fn designation_names(&self, node: Node) -> Vec<&'a str> {
        match node.kind() {
            "identifier" => self.leaf_text(node).into_iter().collect(),
            "tuple_pattern" => self.tuple_pattern_names(node),
            "declaration" | "variable_declaration" => {
                let mut names = Vec::new();
                for i in 0..node.named_child_count() as u32 {
                    if let Some(child) = node.named_child(i) {
                        names.extend(self.designation_names(child));
                    }
                }
                names
            }
            "variable_declarator" => node
                .child_by_field_name("name")
                .and_then(|name| self.leaf_text(name))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The names a destructuring pattern binds, including nested
    /// patterns.
    fn tuple_pattern_names(&self, node: Node) -> Vec<&'a str> {
        let mut names = Vec::new();
        for i in 0..node.named_child_count() as u32 {
            if let Some(child) = node.named_child(i) {
                match child.kind() {
                    "identifier" => names.extend(self.leaf_text(child)),
                    "tuple_pattern" => names.extend(self.tuple_pattern_names(child)),
                    _ => {}
                }
            }
        }
        names
    }

    /// The enclosing declaration of a node: its kind and name, for
    /// diagnostic context. Nested declarations resolve to the
    /// innermost one.
    fn enclosing_item(&self, node: Node) -> (ItemKind, Option<&'a str>) {
        let mut current = node;
        while let Some(parent) = current.parent() {
            let (kind, declarator) = match parent.kind() {
                "namespace_declaration" => (ItemKind::Namespace, false),
                "class_declaration" => (ItemKind::Class, false),
                "struct_declaration" => (ItemKind::Struct, false),
                "interface_declaration" => (ItemKind::Interface, false),
                "record_declaration" | "record_struct_declaration" => (ItemKind::Record, false),
                "enum_declaration" => (ItemKind::Enum, false),
                "delegate_declaration" => (ItemKind::Delegate, false),
                "method_declaration" | "local_function_statement" => (ItemKind::Fn, false),
                "constructor_declaration" => (ItemKind::Constructor, false),
                "destructor_declaration" => (ItemKind::Destructor, false),
                "operator_declaration" | "conversion_operator_declaration" => {
                    (ItemKind::Operator, false)
                }
                "property_declaration" | "indexer_declaration" => (ItemKind::Property, false),
                "event_declaration" => (ItemKind::Event, false),
                // Fields name their first declared variable, matching
                // the parser's declaration model.
                "field_declaration" | "event_field_declaration" => (ItemKind::Const, true),
                _ => {
                    current = parent;
                    continue;
                }
            };
            let name = if declarator {
                self.declarator_names(parent).into_iter().next()
            } else {
                parent
                    .child_by_field_name("name")
                    .and_then(|n| self.leaf_text(n))
            };
            return (kind, name);
        }
        (ItemKind::Other, None)
    }

    /// Text of a name leaf node (an identifier, or a `generic_name`
    /// without its type arguments), or `None` for any other kind.
    fn leaf_text(&self, node: Node) -> Option<&'a str> {
        match node.kind() {
            "identifier" => node.utf8_text(self.bytes).ok(),
            "generic_name" => (0..node.named_child_count() as u32)
                .filter_map(|i| node.named_child(i))
                .find(|child| child.kind() == "identifier")
                .and_then(|identifier| identifier.utf8_text(self.bytes).ok()),
            _ => None,
        }
    }
}

/// Hint at each eligible name, respecting aliases and conditional compilation.
///
/// Missing imports use an alias to avoid guessing namespace/type boundaries.
/// Attribute, import, ambiguous, and shadowed names are exempt.
/// A conditional region exempts guarded code and its whole containing method.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut walker = Walker {
        bytes: parsed.source.as_bytes(),
        relative_roots: HashSet::new(),
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    let root = parsed.syntax_tree().root_node();
    walker.collect_relative_roots(root);
    walker.walk(root);
    walker.diagnostics
}

/// Detect conditional regions before reporting any path in a containing method.
fn contains_conditional(node: Node) -> bool {
    if node.kind().starts_with("preproc_if") {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(contains_conditional)
}

/// True when `frame` binds `short` at `occurrence_start`.
///
/// Counts an import of that name, plus bindings in scope there:
/// declarations anywhere, a local from its position onward.
fn frame_mentions(frame: &ScopeFrame<'_>, short: &str, occurrence_start: usize) -> bool {
    frame.imports.iter().any(|import| import.short == short)
        || frame.bindings.iter().any(|binding| {
            binding.name == short && (binding.start == 0 || binding.start <= occurrence_start)
        })
}

/// True when `frame` binds `short` to something other than
/// `segments`: a local designation, or an import of a different path
/// under the same name.
///
/// Either shadows or ambiguates an imported-name suggestion.
fn frame_shadows(
    frame: &ScopeFrame<'_>,
    short: &str,
    segments: &[&str],
    occurrence_start: usize,
) -> bool {
    frame.bindings.iter().any(|binding| {
        binding.name == short && (binding.start == 0 || binding.start <= occurrence_start)
    }) || frame
        .imports
        .iter()
        .any(|import| import.short == short && import.segments.as_slice() != segments)
}

/// True when `node` is the outermost node of a dotted name chain, so
/// it carries the whole path.
///
/// Inner chain links are covered by their parent: an expression
/// receiver such as `System.Console` inside
/// `System.Console.WriteLine` is a link, never a second occurrence,
/// so call-target receivers are never double-counted.
fn is_chain_head(node: Node) -> bool {
    matches!(node.kind(), "qualified_name" | "member_access_expression")
        && !node.parent().is_some_and(|parent| {
            matches!(parent.kind(), "qualified_name" | "member_access_expression")
        })
}

/// Node kinds that introduce a lexical scope: the file, namespace
/// and type bodies, method-like declarations with parameters,
/// closures, loops, and catch clauses.
fn is_scope(kind: &str) -> bool {
    matches!(
        kind,
        "compilation_unit"
            | "declaration_list"
            | "block"
            | "method_declaration"
            | "constructor_declaration"
            | "local_function_statement"
            | "lambda_expression"
            | "anonymous_method_expression"
            | "for_statement"
            | "foreach_statement"
            | "catch_clause"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;
    use rstest::rstest;

    /// First occurrences provide complete advice, including aliased static calls.
    #[rstest]
    #[case::missing(
        "class C { void M() { System.Console.WriteLine(1); } }",
        "- If `System` is a namespace and the result is clear, add `using System;` at namespace or file scope and use `Console.WriteLine`."
    )]
    #[case::aliased(
        "using Log = System.Console; class C { void M() { System.Console.WriteLine(1); } }",
        "- If clear at the use site, use `Log.WriteLine`; `Log` is already imported."
    )]
    fn check_should_explain_first_occurrence(#[case] source: &str, #[case] advice: &str) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Hint);
        assert_eq!(
            diagnostics[0].message,
            format!(
                "path `System.Console.WriteLine` includes the full namespace.\n\
                 - Shorten with imports or aliases only if the meaning remains clear at the use site.\n\
                 {advice}\n\
                 - Retain namespace or type context when needed; use a type alias if the proposed import targets a containing type, not a namespace.\n\
                 - Keep the full path if shortening would reduce clarity or create a name conflict."
            )
        );
    }

    /// Type advice prefers namespace imports without assuming parents are namespaces.
    #[rstest]
    #[case::string_builder(
        "class C { System.Text.StringBuilder Create() => new(); }",
        "System.Text",
        "StringBuilder"
    )]
    #[case::nested_type(
        "namespace N { class Outer { public class Inner {} } } class C { global::N.Outer.Inner field; }",
        "global::N.Outer",
        "Inner"
    )]
    fn check_should_condition_namespace_advice_on_parent_kind(
        #[case] source: &str,
        #[case] namespace: &str,
        #[case] replacement: &str,
    ) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(&format!(
            "- If `{namespace}` is a namespace and the result is clear, add `using {namespace};` at namespace or file scope and use `{replacement}`."
        )));
        assert!(diagnostics[0].message.contains(
            "use a type alias if the proposed import targets a containing type, not a namespace."
        ));
    }

    /// Static property chains retain their type and property segments.
    #[test]
    fn check_should_keep_static_property_tail_in_replacement() {
        let diagnostics = lint("class C { void M() { System.Console.Out.WriteLine(1); } }");

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("add `using System;`"));
        assert!(
            diagnostics[0]
                .message
                .contains("use `Console.Out.WriteLine`")
        );
    }

    /// Unknown and partial roots stay exempt regardless of depth or type usage.
    #[rstest]
    #[case::type_position("class C { Vendor.Net.Client field; }", 0)]
    #[case::generic_type_position("class C { Vendor.Net.Widget<int> field; }", 0)]
    #[case::namespace(
        "namespace Vendor.Net { class C { void M() { Vendor.Net.Client.Open(); } } }",
        0
    )]
    #[case::file_scoped_namespace(
        "namespace Vendor.Net;\nclass C { void M() { Vendor.Net.Client.Open(); } }",
        0
    )]
    #[case::unknown_receiver("class C { void M() { Vendor.Net.Client.Open(); } }", 0)]
    #[case::ordinary_member("class C { void M() { obj.Member.Open(); } }", 0)]
    #[case::shadowed("class Client {} class C { global::Vendor.Net.Client field; }", 0)]
    #[case::later_shadow("class C { global::Vendor.Net.Client field; } class Client {}", 0)]
    #[case::partial_type("using System; class C { Threading.Tasks.Task field; }", 0)]
    #[case::partial_expression(
        "using System; class C { void M() { Threading.Tasks.Task.Factory.StartNew(); } }",
        0
    )]
    #[case::partial_import(
        "namespace Work { using Threading.Tasks; class C { Threading.Tasks.Task field; } }",
        0
    )]
    #[case::unknown_import("using Vendor.Net; class C { Vendor.Net.Client field; }", 0)]
    #[case::unknown_static_import(
        "using static Vendor.Net.Client; class C { void M() { Vendor.Net.Client.Open(); } }",
        0
    )]
    #[case::nested_namespace(
        "namespace Work { namespace Vendor.Net {} class C { Vendor.Net.Client field; } }",
        0
    )]
    #[case::known_type("class C { System.Threading.Tasks.Task field; }", 1)]
    #[case::microsoft("class C { Microsoft.Win32.RegistryKey field; }", 1)]
    #[case::absolute_custom("class C { global::Vendor.Net.Client field; }", 1)]
    #[case::long_absolute(
        "class C { void M() { global::Vendor.Net.Client.Factory.Instance.Open(); } }",
        1
    )]
    #[case::receiver_shadow(
        "using System.Threading.Tasks; class C { void M(object System) { System.Threading.Tasks.Task.Delay(1); } }",
        0
    )]
    fn check_should_distinguish_custom_roots(#[case] source: &str, #[case] expected: usize) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), expected);
    }

    /// Aliases and relative namespace members are not global roots.
    #[rstest]
    #[case::alias("using System = Vendor.Net; class C { System.Client field; }")]
    #[case::global_using_alias(
        "global using System = Vendor.Net; class C { System.Client field; }"
    )]
    #[case::alias_qualified("using S = System; class C { S::Threading.Tasks.Task field; }")]
    #[case::extern_alias("extern alias System; class C { System::Threading.Tasks.Task field; }")]
    #[case::alias_dotted("using S = System; class C { S.Threading.Tasks.Task field; }")]
    #[case::generic_alias("using System = Vendor.Container<int>; class C { System.Nested field; }")]
    #[case::nested_namespace(
        "namespace Work { namespace System.Threading {} class C { System.Threading.Task field; } }"
    )]
    #[case::separate_namespace(
        "namespace Work.System.Threading {} namespace Work { class C { System.Threading.Task field; } }"
    )]
    #[case::file_scoped_namespace(
        "namespace Work.System; class C { System.Threading.Tasks.Task field; }"
    )]
    #[case::escaped_namespace(
        "namespace Work.@System {} namespace Work { class C { System.Console field; } }"
    )]
    #[case::type_parameter("class C<System> { System.Threading.Tasks.Task field; }")]
    #[case::escaped_binding("class C { void M(object @System) { System.Console.WriteLine(1); } }")]
    #[case::type_binding("class System {} class C { System.Console field; }")]
    #[case::root_import_shadowed(
        "using System; class C { void M(int Console) { System.Console.WriteLine(1); } }"
    )]
    #[case::ambiguous_absolute_alias(
        "using C = global::System.Console; using C = System.Console; class X { global::System.Console field; }"
    )]
    fn check_should_exempt_aliases_and_relative_roots(#[case] source: &str) {
        let diagnostics = lint(source);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    /// Absolute advice keeps its root even when a relative root is shadowed.
    #[rstest]
    #[case::type_path(
        "class C { global::Vendor.Net.Client field; }",
        "global::Vendor.Net.Client",
        "global::Vendor.Net",
        "Client"
    )]
    #[case::expression_path(
        "class C { void M(object System) { global::System.Console.Out.WriteLine(1); } }",
        "global::System.Console.Out.WriteLine",
        "global::System",
        "Console.Out.WriteLine"
    )]
    #[case::relative_import(
        "namespace Work { using Vendor.Net; class C { global::Vendor.Net.Client field; } }",
        "global::Vendor.Net.Client",
        "global::Vendor.Net",
        "Client"
    )]
    fn check_should_preserve_absolute_import_advice(
        #[case] source: &str,
        #[case] path: &str,
        #[case] import: &str,
        #[case] replacement: &str,
    ) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            format!(
                "path `{path}` includes the full namespace.\n\
                 - Shorten with imports or aliases only if the meaning remains clear at the use site.\n\
                 - If `{import}` is a namespace and the result is clear, add `using {import};` at namespace or file scope and use `{replacement}`.\n\
                 - Retain namespace or type context when needed; use a type alias if the proposed import targets a containing type, not a namespace.\n\
                 - Keep the full path if shortening would reduce clarity or create a name conflict."
            )
        );
    }

    /// Generic chains are withheld instead of suggesting code without type arguments.
    #[rstest]
    #[case::type_path("class C { System.Collections.Generic.List<int> field; }")]
    #[case::absolute_type("class C { global::System.Collections.Generic.List<int> field; }")]
    #[case::method("class C { void M() { System.Array.Empty<int>(); } }")]
    #[case::nested_type("class C { global::Vendor.Container<int>.Nested field; }")]
    #[case::imported(
        "using System.Collections.Generic; class C { System.Collections.Generic.List<int> field; }"
    )]
    fn check_should_exempt_generic_chains(#[case] source: &str) {
        let diagnostics = lint(source);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    /// Conditional compilation exempts the containing method, not its sibling.
    #[rstest]
    #[case::body("void M() { System.Console.WriteLine(1);\n#if DEBUG\nint x = 1;\n#endif\n}")]
    #[case::nested("void M() { System.Console.WriteLine(1); {\n#if DEBUG\nint x = 1;\n#endif\n} }")]
    #[case::method("\n#if DEBUG\nvoid M() { System.Console.WriteLine(1); }\n#endif\n")]
    #[case::alternative(
        "\n#if DEBUG\nvoid M() { System.Console.WriteLine(1); }\n#else\nvoid M() { System.Console.WriteLine(2); }\n#endif\n"
    )]
    #[case::local_function(
        "void M() { System.Console.WriteLine(1); void Local() {\n#if DEBUG\nint x = 1;\n#endif\n} }"
    )]
    fn check_should_exempt_conditional_method(#[case] guarded: &str) {
        let source =
            format!("class C {{ {guarded}\nvoid Sibling() {{ System.Console.WriteLine(2); }} }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].item_name.as_deref(), Some("Sibling"));
    }

    /// A region guarding a type exempts all its methods, not the next type.
    #[test]
    fn check_should_exempt_conditionally_compiled_type() {
        let source = "#if DEBUG\nclass C { void M() { System.Console.WriteLine(1); } }\n#endif\nclass D { void Sibling() { System.Console.WriteLine(2); } }";

        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].item_name.as_deref(), Some("Sibling"));
    }

    /// Directive-like text is not a preprocessor region.
    #[rstest]
    #[case::comment("// #if DEBUG\n")]
    #[case::string("var text = \"#if DEBUG\";")]
    #[case::verbatim("var text = @\"#if DEBUG\";")]
    fn check_should_ignore_directive_text(#[case] text: &str) {
        let source = format!("class C {{ void M() {{ {text} System.Console.WriteLine(1); }} }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    /// Run MOD003 over a retained parse of `source`.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = parse(source).expect("test source must parse");
        assert!(!parsed.syntax_tree().root_node().has_error());
        check(&parsed)
    }

    // ── Import available ──

    // Plain using + a qualified occurrence -> one Hint naming the
    // short name at the occurrence's line, carrying the enclosing
    // item's kind and name.
    #[test]
    fn fires_when_fully_qualified_path_is_already_imported() {
        let diags = lint(concat!(
            "using System.Threading.Tasks;\n",
            "class C {\n",
            "    void M() { System.Threading.Tasks.Task.Delay(1); }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_QUALIFIED_PATH);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("M"));
        assert!(
            diags[0]
                .message
                .contains("`System.Threading.Tasks.Task.Delay`")
        );
        assert!(diags[0].message.contains("`Tasks` is already imported"));
        assert!(diags[0].message.contains("use `Task.Delay`"));
    }

    // A qualified type position (return type) counts as one occurrence.
    #[test]
    fn fires_for_qualified_type_positions() {
        let diags = lint(concat!(
            "using global::Nq.Text;\n",
            "class C { global::Nq.Text.Widget Make() { return null; } }\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Text` is already imported"));
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("Make"));
    }

    // A field's qualified type reports the field itself as the
    // enclosing item, matching the parser's first-declared-variable
    // rule.
    #[test]
    fn field_qualified_type_reports_the_field_as_enclosing_item() {
        let diags = lint(concat!(
            "using global::Nq.Text;\n",
            "class C { global::Nq.Text.Widget field; }\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].item_kind, "const");
        assert_eq!(diags[0].item_name.as_deref(), Some("field"));
    }

    // Aliased using -> the hint suggests the alias, not the path's
    // last segment.
    #[test]
    fn fires_with_the_alias_when_using_renames() {
        let diags = lint(concat!(
            "using Widget = global::Nq.Text.Widget;\n",
            "class C {\n",
            "    global::Nq.Text.Widget Make() { return null; }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`global::Nq.Text.Widget`"));
        assert!(diags[0].message.contains("`Widget` is already imported"));
        assert!(diags[0].message.contains("use `Widget`"));
    }

    // An aliased using covering a prefix suggests the alias plus the
    // surviving tail.
    #[test]
    fn fires_with_the_alias_plus_tail_when_an_alias_covers_a_prefix() {
        let diags = lint(concat!(
            "using W = global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() { var a = global::Nq.Text.Widget.Empty; }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`global::Nq.Text.Widget.Empty`"));
        assert!(diags[0].message.contains("`W` is already imported"));
        assert!(diags[0].message.contains("use `W.Empty`"));
    }

    // A file-level using stays in scope inside nested namespaces.
    #[test]
    fn fires_in_nested_namespaces_under_a_file_level_using() {
        let diags = lint(concat!(
            "using global::Nq.Text;\n",
            "namespace Work {\n",
            "    class C { void M() { var a = global::Nq.Text.Widget.Empty; } }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Text` is already imported"));
    }

    // An out-of-scope using does not replace missing-import advice.
    #[test]
    fn check_should_suggest_import_when_existing_using_is_out_of_scope() {
        let diags = lint(concat!(
            "namespace Inner { using global::Nq.Text; }\n",
            "namespace Work {\n",
            "    class C { void M() { var a = global::Nq.Text.Widget.Empty; } }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `using global::Nq;`"));
    }

    // A plain using covering the exact path has no advice: it opens
    // the namespace, and the namespace's own name is not a member.
    #[test]
    fn silent_when_a_plain_using_covers_the_path_exactly() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C { void M() { var a = global::Nq.Text.Widget; } }\n",
        ));

        assert!(diags.is_empty());
    }

    // ── Missing imports ──

    // Every occurrence names its path and a valid alias directive.
    //
    // The receiver chain `System.Console` is a link inside each head,
    // so it never counts as a second occurrence (no double counting).
    #[test]
    fn check_should_hint_at_every_occurrence() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        System.Console.WriteLine(1);\n",
            "        System.Console.WriteLine(2);\n",
            "        System.Console.WriteLine(3);\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("M"));
        assert!(diags[0].message.contains("`System.Console.WriteLine`"));
        assert!(diags[0].message.contains("add `using System;`"));
    }

    // Existing imports affect advice, not the number of hints.
    #[test]
    fn check_should_hint_at_every_imported_occurrence() {
        let diags = lint(concat!(
            "using System.Threading.Tasks;\n",
            "class C {\n",
            "    void M() {\n",
            "        System.Threading.Tasks.Task.Delay(1);\n",
            "        System.Threading.Tasks.Task.Delay(2);\n",
            "        System.Threading.Tasks.Task.Delay(3);\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 3);
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("`Tasks` is already imported"))
        );
        assert!(diags.iter().all(|d| !d.message.contains("times")));
    }

    // Hints follow occurrence order across different paths.
    #[test]
    fn check_should_order_hints_by_occurrence() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        System.Console.WriteLine(1);\n",
            "        System.Console.WriteLine(2);\n",
            "        System.Console.WriteLine(3);\n",
            "        System.Math.Max(1, 2);\n",
            "        System.Math.Max(1, 2);\n",
            "        System.Math.Max(1, 2);\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 6);
        assert!(diags[0].message.contains("`System.Console.WriteLine`"));
        assert!(diags[3].message.contains("`System.Math.Max`"));
        assert!(diags[0].line < diags[1].line);
    }

    // Same-line hints follow source order, not alphabetical path order.
    #[test]
    fn check_should_order_same_line_hints_by_occurrence() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        var t = (System.Math.Abs(-1), System.Console.ReadKey());\n",
            "        var u = (System.Math.Abs(-1), System.Console.ReadKey());\n",
            "        var v = (System.Math.Abs(-1), System.Console.ReadKey());\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 6);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[1].line, 3);
        assert!(diags[0].message.contains("`System.Math.Abs`"));
        assert!(diags[1].message.contains("`System.Console.ReadKey`"));
    }

    // Multi-segment chains without a rooted first segment (member
    // receivers) never count.
    #[test]
    fn silent_for_paths_without_a_rooted_first_segment() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        Deep.Member.Op();\n",
            "        Deep.Member.Op();\n",
            "        Deep.Member.Op();\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // ── Exemptions (contract D7) ──

    // Attribute spans never count, including the attribute's qualified name.
    #[test]
    fn silent_inside_attribute_list_spans() {
        let diags = lint(concat!(
            "using global::Nq;\n",
            "[global::Nq.Probe.Mark]\n",
            "class A { }\n",
            "[global::Nq.Probe.Mark]\n",
            "class B { }\n",
            "[global::Nq.Probe.Mark]\n",
            "class C { }\n",
        ));

        assert!(diags.is_empty());
    }

    // `using` directive text itself is never an occurrence, in any of
    // its shapes.
    #[test]
    fn using_directive_text_is_never_flagged() {
        let diags = lint(concat!(
            "using System.Text;\n",
            "using global::Nq.Text.Widget;\n",
            "using W = global::Nq.Text.Widget;\n",
            "using static System.Math;\n",
            "class C { }\n",
        ));

        assert!(diags.is_empty());
    }

    // Shadowing the referenced name suppresses advice.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_parameter_or_local() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void P(int Empty) { var a = global::Nq.Text.Widget.Empty; }\n",
            "    void L() { var Empty = 1; var b = global::Nq.Text.Widget.Empty; }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A local shadows only from its position: the occurrence before it
    // still fires, the one after stays silent.
    #[test]
    fn fires_before_and_suppresses_after_a_shadowing_local() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() {\n",
            "        var a = global::Nq.Text.Widget.Empty;\n",
            "        var Empty = 1;\n",
            "        var b = global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
        assert!(diags[0].message.contains("`Widget` is already imported"));
    }

    // A foreach or catch designation binds its name for the whole
    // statement, so the occurrences after it stay silent.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_foreach_or_catch_designation() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void F() {\n",
            "        foreach (var Empty in items) { var a = global::Nq.Text.Widget.Empty; }\n",
            "    }\n",
            "    void H() {\n",
            "        try { } catch (Exception Empty) { var b = global::Nq.Text.Widget.Empty; }\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A lambda parameter binds inside the lambda body.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_lambda_parameter() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() {\n",
            "        Func<int, int> f = Empty => global::Nq.Text.Widget.Empty.Value;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A same-named declaration in the file scope shadows the name the
    // advice references.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_same_named_declaration() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class Empty { }\n",
            "class C { void M() { var a = global::Nq.Text.Widget.Empty; } }\n",
        ));

        assert!(diags.is_empty());
    }

    /// Root namespace imports supply partially qualified replacements.
    #[rstest]
    #[case::system(
        "using System; class C { void M() { System.Console.WriteLine(1); } }",
        "Console.WriteLine",
        "System"
    )]
    #[case::absolute(
        "using global::Nq; class C { void M() { var value = global::Nq.Text.Widget.Empty; } }",
        "Text.Widget.Empty",
        "Nq"
    )]
    #[case::global_using(
        "global using System; class C { System.Text.StringBuilder Create() => new(); }",
        "Text.StringBuilder",
        "System"
    )]
    fn check_should_reuse_root_namespace_import(
        #[case] source: &str,
        #[case] replacement: &str,
        #[case] imported: &str,
    ) {
        let diags = lint(source);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(&format!(
            "- If clear at the use site, use `{replacement}`; `{imported}` is already imported."
        )));
    }

    // Shadowed imports never fall back to missing-import advice.
    #[test]
    fn check_should_suppress_advice_when_covering_import_is_shadowed() {
        let diags = lint(concat!(
            "using System.Threading.Tasks;\n",
            "class C {\n",
            "    void M(int Task) {\n",
            "        System.Threading.Tasks.Task.Delay(1);\n",
            "        System.Threading.Tasks.Task.Delay(2);\n",
            "        System.Threading.Tasks.Task.Delay(3);\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A destructuring declaration binds every pattern name.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_deconstruction_pattern() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() {\n",
            "        var (d, Empty) = (1, 2);\n",
            "        var a = global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // An `is` pattern's designation binds its name from the pattern
    // onward.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_pattern_designation() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M(object o) {\n",
            "        if (o is int Empty) { var a = global::Nq.Text.Widget.Empty; }\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // Out-arguments, positional-pattern sub-designations, and
    // parenthesized case designations bind their names like locals.
    #[test]
    fn silent_when_short_name_is_shadowed_by_out_var_and_positional_designations() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void P() { Try(out var Empty, out var b); var a = global::Nq.Text.Widget.Empty; }\n",
            "    void Q(object o) { if (o is Pair(int a, int Empty)) { var b = global::Nq.Text.Widget.Empty; } }\n",
            "    void R(object o) { switch (o) { case var (a, Empty): break; } var c = global::Nq.Text.Widget.Empty; }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // Query range variables (`from`, `let`, `into`, `join`) bind their
    // names for the rest of the query.
    #[test]
    fn silent_when_short_name_is_shadowed_by_query_variables() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void F() {\n",
            "        var q = from Empty in xs select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void L() {\n",
            "        var q = from x in xs let Empty = x select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void I() {\n",
            "        var q = from x in xs select x into Empty select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void J() {\n",
            "        var q = from x in xs join Empty in ys on x.K equals ys.K select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void K() {\n",
            "        var q = from x in xs join y in ys on x.K equals y.K into Empty select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A typed join's range variable - not its type - binds the name.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_typed_join_range_variable() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void T() {\n",
            "        var q = from x in xs join Foo Empty in ys on x.K equals ys.K select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // Renaming only the join binder flips the occurrence to firing.
    #[test]
    fn fires_when_only_the_join_binder_is_renamed() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void R() {\n",
            "        var q = from x in xs join y in ys on x.K equals y.K select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_QUALIFIED_PATH);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert!(diags[0].message.contains("`Widget` is already imported"));
    }

    // A top-level declaration also binds its name in the file scope,
    // so the advised short name would conflict.
    #[test]
    fn silent_when_a_top_level_declaration_shares_the_short_name() {
        let diags = lint(concat!(
            "using global::Nq;\n",
            "class Util { }\n",
            "class C {\n",
            "    void M() {\n",
            "        global::Nq.Util.Helper.Run();\n",
            "        global::Nq.Util.Helper.Run();\n",
            "        global::Nq.Util.Helper.Run();\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A top-level import binding the same short name also blocks the
    // advice: the short name already resolves to that import's path.
    #[test]
    fn silent_when_a_top_level_using_shares_the_short_name() {
        let diags = lint(concat!(
            "using Foo = Other.Ns.Foo;\n",
            "class C {\n",
            "    void M() {\n",
            "        var a = Microsoft.Foo.Text.Value;\n",
            "        var b = Microsoft.Foo.Text.Value;\n",
            "        var c = Microsoft.Foo.Text.Value;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // An exact namespace cover and a conflicting using never manufacture advice.
    #[test]
    fn silent_when_short_name_is_ambiguous_across_usings() {
        let diags = lint(concat!(
            "using global::Na.X;\n",
            "using global::Nb.X;\n",
            "class C {\n",
            "    void M() { var a = global::Na.X; var b = global::Na.X; var c = global::Na.X; }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // ── Static imports ──

    // Static usings do not supply a known imported name.
    #[test]
    fn check_should_suggest_alias_when_only_static_using_exists() {
        let diags = lint(concat!(
            "using static global::Nq.Thing;\n",
            "class C { void M() { global::Nq.Thing.Op(1); } }\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `using global::Nq;`"));
    }
}
