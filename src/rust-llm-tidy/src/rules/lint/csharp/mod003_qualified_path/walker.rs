//! Scope-tracking traversal for the qualified-name rule.
//!
//! [`Walker::walk`] maintains the scope stack, records declarations and
//! usings before a scope's children, and turns each eligible chain
//! head into a hint through `suggestion_under`.

use super::ROOT_SEGMENTS;
use super::names::{declarator_names, local_bindings, name_segments};
use super::scope::{Binding, Import, ScopeFrame, covering_import, frame_mentions, frame_shadows};
use super::syntax::{
    contains_conditional, is_chain_head, is_scope, leaf_text, leftmost_name, leftmost_segment,
};
use super::usings::collect_usings;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_QUALIFIED_PATH;
use crate::source::ItemKind;
use std::collections::HashSet;
use tree_sitter::Node;

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
pub(super) struct Walker<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) relative_roots: HashSet<&'a str>,
    pub(super) scopes: Vec<ScopeFrame<'a>>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

impl<'a> Walker<'a> {
    /// Withhold unprefixed roots declared below another namespace anywhere in the file.
    ///
    /// Separate namespace blocks can contribute members to the same namespace,
    /// so lexical scope alone cannot exclude relative resolution.
    pub(super) fn collect_relative_roots(&mut self, node: Node) {
        if matches!(
            node.kind(),
            "namespace_declaration" | "file_scoped_namespace_declaration"
        ) && let Some(name) = node.child_by_field_name("name")
            && let Some(segments) = name_segments(name, self.bytes)
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
    pub(super) fn walk(&mut self, node: Node) {
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
        let imports = collect_usings(self.bytes, node);
        let alias = node
            .child_by_field_name("name")
            .and_then(|n| leaf_text(n, self.bytes));

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
                .and_then(|n| leftmost_segment(n, self.bytes))
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
                .and_then(|n| leaf_text(n, self.bytes))
                .into_iter()
                .collect(),
            // Fields and event fields declare their variables in
            // declarator children; the names are visible throughout
            // the type body.
            "field_declaration" | "event_field_declaration" => declarator_names(self.bytes, node),
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
        let names = local_bindings(self.bytes, node);
        if let Some(frame) = self.scopes.last_mut() {
            frame.bindings.extend(
                names
                    .into_iter()
                    .map(|(name, start)| Binding { name, start }),
            );
        }
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

        let Some(segments) = name_segments(node, self.bytes) else {
            return; // not a plain dotted chain: file-local text cannot decide
        };
        let absolute = leftmost_name(node).is_some_and(|n| n.kind() == "alias_qualified_name");
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
        let covering = covering_import(&self.scopes, &segments, absolute);
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
                declarator_names(self.bytes, parent).into_iter().next()
            } else {
                parent
                    .child_by_field_name("name")
                    .and_then(|n| leaf_text(n, self.bytes))
            };
            return (kind, name);
        }
        (ItemKind::Other, None)
    }
}
