//! Scope-tracking traversal for the qualified-path rule.
//!
//! [`Walker::walk`] maintains the scope stack, records names and
//! imports before a scope's children, and turns each eligible chain
//! head into a hint through `suggestion_under`.

use super::imports::{collect_use, use_path};
use super::scope::{
    Binding, Import, ScopeFrame, covering_import, frame_mentions, frame_shadows, is_full_path,
};
use super::syntax::{
    contains_conditional, has_conditional_attribute, is_chain_head, is_conditional_attribute,
    is_scope, leaf_text, scoped_segments,
};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_QUALIFIED_PATH;
use crate::source::ItemKind;
use tree_sitter::Node;

/// What an existing import advises: the imported short name
/// and the replacement text for the whole path.
///
/// An exact import suggests the short name alone. A path extending an
/// imported prefix (`std::sync::Arc::new` under `use std::sync::Arc;`)
/// suggests the short name plus the remaining segments (`Arc::new`).
struct Suggestion<'a> {
    short: &'a str,
    replacement: String,
}

/// Lexical scopes and hints collected in source order.
pub(super) struct Walker<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) scopes: Vec<ScopeFrame<'a>>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

impl<'a> Walker<'a> {
    /// Walk `node`'s subtree: maintain scope frames, record
    /// qualified-path occurrences, and never descend into exempt spans.
    pub(super) fn walk(&mut self, node: Node) {
        if has_conditional_attribute(node, self.bytes)
            || (node.kind() == "function_item" && contains_conditional(node, self.bytes))
            || (matches!(node.kind(), "source_file" | "declaration_list" | "block")
                && (0..node.named_child_count() as u32)
                    .filter_map(|i| node.named_child(i))
                    .any(|child| {
                        child.kind() == "inner_attribute_item"
                            && is_conditional_attribute(child, self.bytes)
                    }))
        {
            return;
        }

        match node.kind() {
            // D7 exemptions: `use` text, macro spans, and attribute
            // items produce no occurrences; a `use` contributes its
            // imports to the current scope instead.
            "use_declaration" => return,

            "attribute_item" | "inner_attribute_item" | "macro_invocation" | "macro_definition" => {
                return;
            }
            _ => {}
        }

        let pushed = if is_scope(node.kind()) {
            self.scopes.push(ScopeFrame {
                module_scope: node.kind() == "source_file"
                    || node
                        .parent()
                        .is_some_and(|parent| parent.kind() == "mod_item"),
                ..ScopeFrame::default()
            });
            // Items and imports are visible before their declaration.
            for i in 0..node.named_child_count() as u32 {
                if let Some(child) = node.named_child(i) {
                    self.record_item_name(child);
                    if child.kind() == "use_declaration" {
                        if has_conditional_attribute(child, self.bytes) {
                            self.record_conditional_use(child);
                        } else {
                            self.record_use(child);
                        }
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

    /// Add a `use` declaration's imports to the innermost scope frame.
    fn record_use(&mut self, node: Node) {
        let (imports, globs) = collect_use(self.bytes, node);
        if let Some(frame) = self.scopes.last_mut() {
            frame.imports.extend(imports);
            frame.has_glob |= !globs.is_empty();
        }
    }

    /// Reserve a cfg-guarded `use`'s bound names as scope mentions.
    ///
    /// The guarded import may be absent, so it never covers a path. Its
    /// short name stays reserved, so missing-import advice cannot advise
    /// a duplicate of it.
    fn record_conditional_use(&mut self, node: Node) {
        let (imports, globs) = collect_use(self.bytes, node);
        if let Some(frame) = self.scopes.last_mut() {
            frame.has_glob |= !globs.is_empty();
            frame
                .bindings
                .extend(imports.into_iter().map(|import| Binding {
                    name: import.short,
                    start: 0,
                }));
        }
    }

    /// Record an item node's own name into the current scope frame.
    ///
    /// Items (`fn`, `struct`, ...) bind their name in the enclosing
    /// scope, order-independent. Callers run this before the item's own
    /// frame is pushed, so `fn` names land around the function, not
    /// inside it.
    fn record_item_name(&mut self, node: Node) {
        if node.kind() == "extern_crate_declaration" {
            let name = node
                .child_by_field_name("alias")
                .or_else(|| node.child_by_field_name("name"))
                .and_then(|name| leaf_text(name, self.bytes));

            if let Some(name) = name {
                let conditional = has_conditional_attribute(node, self.bytes);
                if let Some(frame) = self.scopes.last_mut() {
                    if conditional {
                        frame.bindings.push(Binding { name, start: 0 });
                    } else {
                        frame.external_crates.push(name);
                    }
                }
            }
            return;
        }

        let name = match node.kind() {
            "function_item" | "struct_item" | "enum_item" | "union_item" | "type_item"
            | "trait_item" | "const_item" | "static_item" | "mod_item" | "macro_definition" => node
                .child_by_field_name("name")
                .and_then(|n| leaf_text(n, self.bytes)),
            _ => return,
        };
        if let Some(name) = name
            && let Some(frame) = self.scopes.last_mut()
        {
            frame.bindings.push(Binding { name, start: 0 });
        }
    }

    /// Record the pattern names `node` binds into the current scope
    /// frame.
    ///
    /// Local patterns (`let`, parameters, loop and match arms) bind
    /// from their position onward. A `match_pattern` wrapper's
    /// identifier descendants cover the names; `closure_parameters`
    /// holds its patterns as direct children (`|a, b|`).
    fn record_bindings(&mut self, node: Node) {
        let mut names: Vec<(&'a str, usize)> = Vec::new();
        match node.kind() {
            "type_parameter" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|name| leaf_text(name, self.bytes))
                {
                    names.push((name, 0));
                }
            }
            "let_declaration" | "let_condition" | "parameter" | "for_expression" | "match_arm"
            | "closure_parameters" => {
                if let Some(pattern) = node.child_by_field_name("pattern") {
                    names.extend(
                        self.pattern_names(pattern)
                            .into_iter()
                            .map(|name| (name, node.start_byte())),
                    );
                } else {
                    for i in 0..node.named_child_count() as u32 {
                        if let Some(pattern) = node.named_child(i) {
                            names.extend(
                                self.pattern_names(pattern)
                                    .into_iter()
                                    .map(|name| (name, node.start_byte())),
                            );
                        }
                    }
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

    /// Suggest a readable replacement for one eligible qualified path.
    fn record_occurrence(&mut self, node: Node) {
        let Some(segments) = scoped_segments(node, self.bytes) else {
            return; // not a plain `::` path: file-local text cannot decide
        };
        if !is_full_path(&self.scopes, &segments, node.start_byte()) {
            return;
        }

        let line = node.start_position().row + 1;
        let (kind, name) = self.enclosing_item(node);
        let covering = covering_import(&self.scopes, &segments);
        let path = segments.join("::");
        let advice = if let Some(matched) = covering {
            self.suggestion_under(matched, &segments, node.start_byte())
                .map(|suggestion| {
                    format!(
                        "- If clear at the call site, use `{}`; `{}` is already imported.",
                        suggestion.replacement, suggestion.short
                    )
                })
        } else {
            let imported = use_path(&path);
            let short = imported.rsplit("::").next().expect("nonempty path");
            let shadowed = self
                .scopes
                .iter()
                .any(|frame| frame_mentions(frame, short, node.start_byte()));
            (!shadowed).then(|| {
                let replacement = &path[imported.len() - short.len()..];
                format!("- If clear at the call site, add `use {imported};` at module scope and use `{replacement}`.")
            })
        };

        if let Some(advice) = advice {
            self.diagnostics.push(Diagnostic {
                title: Some("full namespace qualification in code".into()),
                severity: Severity::Hint,
                code: CODE_QUALIFIED_PATH,
                message: format!(
                    "path `{path}` includes the full namespace.\n\
                     Why: full namespace prefixes give readers longer lines to scan before reaching the item name, making code harder to understand.\n\
                     Suggestions:\n\
                     {advice}\n\
                     - Import a parent module if the bare name loses context: for example, import `std::process` and use `process::id()`, not `id()`.\n\
                     - Keep the full path if shortening would reduce clarity or create a name conflict.\n\
                     - Verify the shorter path resolves to the same item; this hint uses syntax, not compiler name resolution."
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
    /// Suppresses when the innermost frame mentioning the imported
    /// name shadows or ambiguates it.
    ///
    /// Such mentions are a local binding, parameter, item, or an
    /// import of a different path under the same name. A `let`
    /// positioned after the occurrence is not yet a mention.
    fn suggestion_under(
        &self,
        matched: &Import<'a>,
        segments: &[&str],
        occurrence_start: usize,
    ) -> Option<Suggestion<'a>> {
        let short = matched.short;
        self.scopes
            .iter()
            .rev()
            .find(|frame| frame_mentions(frame, short, occurrence_start))
            .filter(|frame| !frame_shadows(frame, short, &matched.segments, occurrence_start))?;
        let tail = &segments[matched.segments.len()..];
        let replacement = if tail.is_empty() {
            short.to_string()
        } else {
            format!("{short}::{}", tail.join("::"))
        };
        Some(Suggestion { short, replacement })
    }

    /// All identifier names a pattern node binds.
    ///
    /// Destructuring contributes every bound identifier. A typed
    /// tuple-struct path (`Some(x)`) also contributes its path
    /// identifier, over-suppressing where the file alone cannot decide.
    fn pattern_names(&self, pattern: Node) -> Vec<&'a str> {
        let mut names = Vec::new();
        if let Some(name) = leaf_text(pattern, self.bytes) {
            names.push(name);
            return names;
        }
        for i in 0..pattern.named_child_count() as u32 {
            if let Some(child) = pattern.named_child(i) {
                names.extend(self.pattern_names(child));
            }
        }
        names
    }

    /// The enclosing item of a node: its kind and name, for diagnostic
    /// context. Nested items resolve to the innermost one.
    fn enclosing_item(&self, node: Node) -> (ItemKind, Option<&'a str>) {
        let mut current = node;
        while let Some(parent) = current.parent() {
            let (kind, name_field) = match parent.kind() {
                "function_item" => (ItemKind::Fn, "name"),
                "struct_item" => (ItemKind::Struct, "name"),
                "enum_item" => (ItemKind::Enum, "name"),
                "union_item" => (ItemKind::Union, "name"),
                "type_item" => (ItemKind::Type, "name"),
                "trait_item" => (ItemKind::Trait, "name"),
                "const_item" => (ItemKind::Const, "name"),
                "static_item" => (ItemKind::Static, "name"),
                "mod_item" => (ItemKind::Mod, "name"),
                "impl_item" => (ItemKind::Impl, ""),
                "macro_definition" => (ItemKind::Macro, "name"),
                _ => {
                    current = parent;
                    continue;
                }
            };
            let name = if name_field.is_empty() {
                None
            } else {
                parent
                    .child_by_field_name(name_field)
                    .and_then(|n| leaf_text(n, self.bytes))
            };
            return (kind, name);
        }
        (ItemKind::Other, None)
    }
}
