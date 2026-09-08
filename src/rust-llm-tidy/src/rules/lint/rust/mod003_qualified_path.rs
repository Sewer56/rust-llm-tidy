//! `MOD003`: shorten qualified paths to make code easier to read.
//!
//! A qualified path spells out where a name lives, such as `std::sync::Arc`.
//! This rule reads the file's syntax and emits hints; it does not rewrite code
//! or ask the compiler to resolve names.
//!
//! # Explanation 1: reuse an existing import
//!
//! An import gives a long path a short name.
//!
//! The walker tracks imports and names in nested scopes, then looks for the
//! longest import matching the beginning of a path.
//!
//! ```rust
//! use std::sync::Arc;
//!
//! let before = std::sync::Arc::new(42);
//! let after = Arc::new(42);
//! ```
//!
//! Here, `use std::sync::Arc;` already supplies `Arc`, so the hint replaces
//! only `std::sync::Arc`; `::new` stays. An import alias works the same way:
//! `use std::sync::Arc as Shared;` makes the replacement `Shared::new(42)`.
//!
//! # Explanation 2: suggest a missing import
//!
//! Without a matching import, the rule can suggest adding one at module scope.
//!
//! It uses naming conventions: the first uppercase segment is treated as a
//! type or trait, so later segments stay on the replacement.
//!
//! ```rust
//! // Before: no import is needed for the long spelling.
//! let before = std::sync::Arc::new(42);
//! ```
//!
//! ```rust
//! // After: import the type, not its associated function `new`.
//! use std::sync::Arc;
//!
//! let after = Arc::new(42);
//! ```
//!
//! # Code walkthrough: start at `check`
//!
//! Read these functions in call order, not their order in the file.
//!
//! 1. [`check`] receives an already-parsed file. It creates a [`Walker`], visits
//!    the syntax tree, and returns the collected diagnostics.
//! 2. [`Walker::collect_roots`] gathers possible path beginnings from imports.
//!    These join [`ROOT_SEGMENTS`], such as `std` and `crate`. This first pass
//!    lets an import below a use site contribute evidence above it.
//! 3. [`Walker::walk`] visits syntax nodes recursively. Entering a scope pushes
//!    a [`ScopeFrame`]; leaving a nested scope pops it.
//! 4. [`is_chain_head`] selects the outermost node of a path. For
//!    `std::sync::Arc::new`, this avoids separate hints for each shorter prefix.
//!    [`Walker::record_occurrence`] splits that node into segments and checks
//!    whether the path is eligible.
//! 5. [`Walker::covering_import`] finds the longest visible import prefix.
//!    [`Walker::suggestion_under`] builds a replacement when that import's
//!    short name is usable.
//! 6. [`Walker::record_occurrence`] adds a hint only when it has advice.
//!    [`Walker::enclosing_item`] supplies the containing item's kind and name;
//!    the path node supplies the line number. [`check`] returns these hints.
//!
//! ## What the stored data means
//!
//! - [`Walker`]: source bytes, known roots, active scopes, and accumulated hints
//! - [`ScopeFrame`]: imports and bound names in one active scope
//! - [`Import`]: a full imported path and its short name or alias
//! - [`Binding`]: a declared name and the position where it starts counting
//! - [`Suggestion`]: the short name used and the replacement text
//!
//! The scope stack models nested visibility, not compiler name resolution.
//!
//! [`frame_mentions`] asks whether a scope uses a name; [`frame_shadows`] asks
//! whether it conflicts with the proposed import.
//!
//! Imports and item names are collected before visiting a scope's children.
//! Their declarations can appear after their uses. A binding's `start` value
//! distinguishes scope-wide items from locals that count only from their position.
//!
//! Without a covering import, [`use_path`] chooses what to import, as shown
//! in Explanation 2.
//!
//! ## Trace the first example
//!
//! `collect_use` turns `use std::sync::Arc;` into an import whose short name is
//! `Arc`.
//!
//! `scoped_segments` turns the call's path into `std`, `sync`, `Arc`, `new`.
//!
//! The covering import matches the first three segments. `suggestion_under`
//! joins `Arc` to the remaining `new`, producing `Arc::new`.
//!
//! Next: open [`check`], then follow [`Walker::walk`] to
//! [`Walker::record_occurrence`] with this example in mind.
//!
//! # Remarks
//!
//! Hints are withheld for conflicting short names and exempt syntax, including
//! imports, macros, attributes, and conditionally compiled regions or functions.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_QUALIFIED_PATH;
use crate::source::{ItemKind, ParseResult};
use std::collections::HashSet;
use tree_sitter::Node;

/// First path segments that always root a fully-qualified path: the
/// module roots and the standard crates.
///
/// Imported roots and lowercase crate/module names are also eligible.
const ROOT_SEGMENTS: &[&str] = &["crate", "self", "super", "std", "core", "alloc"];

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
struct Walker<'a> {
    bytes: &'a [u8],
    roots: HashSet<&'a str>,
    scopes: Vec<ScopeFrame<'a>>,
    diagnostics: Vec<Diagnostic>,
}

/// The imports and names one lexical scope introduces during the walk.
#[derive(Default)]
struct ScopeFrame<'a> {
    imports: Vec<Import<'a>>,
    bindings: Vec<Binding<'a>>,
}

/// One name a lexical scope binds: a local binding (`let`,
/// parameter, pattern) or a same-scope item.
///
/// `start == 0` marks an item: visible throughout its scope. Any
/// other `start` is the binding's byte offset; a `let` shadows only
/// from that position onward.
struct Binding<'a> {
    name: &'a str,
    start: usize,
}

/// One explicit `use` import: the full imported path plus the name it
/// binds in scope (the alias for `use a::b::C as D`).
struct Import<'a> {
    segments: Vec<&'a str>,
    short: &'a str,
}

impl<'a> Walker<'a> {
    /// Collect the first segments of every `use` path (imports and
    /// globs) into the root set.
    fn collect_roots(&mut self, node: Node) {
        if node.kind() == "use_declaration" {
            let (imports, globs) = self.collect_use(node);
            for import in &imports {
                if let Some(first) = import.segments.first() {
                    self.roots.insert(first);
                }
            }
            for glob in &globs {
                if let Some(first) = glob.first() {
                    self.roots.insert(first);
                }
            }
            return;
        }
        for i in 0..node.named_child_count() as u32 {
            if let Some(child) = node.named_child(i) {
                self.collect_roots(child);
            }
        }
    }

    /// Walk `node`'s subtree: maintain scope frames, record
    /// qualified-path occurrences, and never descend into exempt spans.
    fn walk(&mut self, node: Node) {
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
            self.scopes.push(ScopeFrame::default());
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
        let (imports, _globs) = self.collect_use(node);
        if let Some(frame) = self.scopes.last_mut() {
            frame.imports.extend(imports);
        }
    }

    /// Reserve a cfg-guarded `use`'s bound names as scope mentions.
    ///
    /// The guarded import may be absent, so it never covers a path. Its
    /// short name stays reserved, so missing-import advice cannot advise
    /// a duplicate of it.
    fn record_conditional_use(&mut self, node: Node) {
        let (imports, _globs) = self.collect_use(node);
        if let Some(frame) = self.scopes.last_mut() {
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
        let name = match node.kind() {
            "function_item" | "struct_item" | "enum_item" | "union_item" | "type_item"
            | "trait_item" | "const_item" | "static_item" | "mod_item" | "macro_definition" => node
                .child_by_field_name("name")
                .and_then(|n| self.leaf_text(n)),
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
        let Some(segments) = self.scoped_segments(node) else {
            return; // not a plain `::` path: file-local text cannot decide
        };
        if segments.len() < 2
            || matches!(
                segments[0],
                "str"
                    | "bool"
                    | "char"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "f32"
                    | "f64"
            )
            || (!self.roots.contains(segments[0])
                && !segments[0].starts_with(|c: char| c.is_ascii_lowercase()))
        {
            return;
        }

        let line = node.start_position().row + 1;
        let (kind, name) = self.enclosing_item(node);
        let covering = self.covering_import(&segments);
        let path = segments.join("::");
        let advice = if let Some(matched) = covering {
            self.suggestion_under(matched, &segments, node.start_byte())
                .map(|suggestion| {
                    format!(
                        "- Replace this path with `{}`; `{}` is already imported.",
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
                format!("- Add `use {imported};` at module scope.\n- Replace this path with `{replacement}`.")
            })
        };

        if let Some(advice) = advice {
            self.diagnostics.push(Diagnostic {
                severity: Severity::Hint,
                code: CODE_QUALIFIED_PATH,
                message: format!(
                    "fully-qualified path `{path}` makes code harder to read.\n{advice}"
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

    /// The longest in-scope import covering `segments`: its path equals
    /// the occurrence path or prefixes it.
    ///
    /// The innermost frame wins ties, since its binding shadows outer
    /// ones. One-segment imports never cover: their suggestion would
    /// repeat the occurrence verbatim (`use std;` covering `std::mem`).
    fn covering_import(&self, segments: &[&str]) -> Option<&Import<'a>> {
        let mut covering: Option<&Import<'a>> = None;
        for frame in self.scopes.iter().rev() {
            for import in &frame.imports {
                let covers =
                    import.segments.len() >= 2 && segments.starts_with(import.segments.as_slice());
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

    /// The explicit imports and glob prefixes of one `use` declaration.
    ///
    /// Groups flatten to one import per member (`use a::{B, C}`
    /// imports `a::B` and `a::C`); an alias binds the alias name. Glob
    /// prefixes return separately: they root paths but never enable
    /// imported-name advice.
    fn collect_use(&self, node: Node) -> (Vec<Import<'a>>, Vec<Vec<&'a str>>) {
        let mut imports = Vec::new();
        let mut globs = Vec::new();
        if let Some(argument) = node.child_by_field_name("argument") {
            self.collect_use_argument(argument, &[], &mut imports, &mut globs);
        }
        (imports, globs)
    }

    /// Collect imports from one `use` argument node under `prefix`.
    fn collect_use_argument(
        &self,
        node: Node,
        prefix: &[&'a str],
        imports: &mut Vec<Import<'a>>,
        globs: &mut Vec<Vec<&'a str>>,
    ) {
        match node.kind() {
            // Bare path: `use std;`. `self` inside a group binds the
            // group's prefix path itself (`use a::b::{self}` imports
            // `a::b` as `b`).
            "identifier" | "crate" | "self" | "super" => {
                if let Some(text) = self.leaf_text(node) {
                    if text == "self" && !prefix.is_empty() {
                        let short = prefix[prefix.len() - 1];
                        imports.push(Import {
                            segments: prefix.to_vec(),
                            short,
                        });
                    } else {
                        self.push_import(imports, prefix, &[text], text);
                    }
                }
            }
            "scoped_identifier" => {
                if let Some(segments) = self.scoped_segments(node) {
                    let short = *segments.last().expect("scoped path has a name");
                    self.push_import(imports, prefix, &segments, short);
                }
            }
            "use_as_clause" => {
                let alias = node
                    .child_by_field_name("alias")
                    .and_then(|n| self.leaf_text(n));
                if let (Some(alias), Some(segments)) = (
                    alias,
                    node.child_by_field_name("path")
                        .and_then(|p| self.path_argument_segments(p)),
                ) {
                    self.push_import(imports, prefix, &segments, alias);
                }
            }
            "use_list" => {
                for i in 0..node.named_child_count() as u32 {
                    if let Some(child) = node.named_child(i) {
                        self.collect_use_argument(child, prefix, imports, globs);
                    }
                }
            }
            "scoped_use_list" => {
                if let (Some(prefix_path), Some(list)) = (
                    node.child_by_field_name("path")
                        .and_then(|p| self.path_argument_segments(p)),
                    node.child_by_field_name("list"),
                ) {
                    let mut nested = prefix.to_vec();
                    nested.extend(prefix_path.iter().copied());
                    self.collect_use_argument(list, &nested, imports, globs);
                }
            }
            "use_wildcard" => {
                let mut glob = prefix.to_vec();
                if let Some(child) = node.named_child(0)
                    && let Some(segments) = self.path_argument_segments(child)
                {
                    glob.extend(segments.iter().copied());
                }
                globs.push(glob);
            }
            _ => {}
        }
    }

    /// Append `inner` under `prefix` as one import binding `short`.
    fn push_import(
        &self,
        imports: &mut Vec<Import<'a>>,
        prefix: &[&'a str],
        inner: &[&'a str],
        short: &'a str,
    ) {
        let mut segments = prefix.to_vec();
        segments.extend(inner.iter().copied());
        imports.push(Import { segments, short });
    }

    /// The segments of a `use` argument path node: a scoped chain or a
    /// bare leaf (`crate`, `self`, `super`, an identifier).
    fn path_argument_segments(&self, node: Node) -> Option<Vec<&'a str>> {
        match node.kind() {
            "scoped_identifier" => self.scoped_segments(node),
            "identifier" | "crate" | "self" | "super" => self.leaf_text(node).map(|t| vec![t]),
            _ => None,
        }
    }

    /// The `::`-joined segments of a scoped path node, root first.
    ///
    /// Returns `None` when the chain does not bottom out in plain
    /// segments (a generic or bracketed root such as
    /// `<T as Trait>::Assoc`). The path text then cannot be decided
    /// file-locally.
    fn scoped_segments(&self, node: Node) -> Option<Vec<&'a str>> {
        let mut segments = Vec::new();
        let mut current = node;
        loop {
            let name = current.child_by_field_name("name")?;
            segments.push(self.leaf_text(name)?);
            match current.child_by_field_name("path") {
                None => break,
                Some(path) => match path.kind() {
                    "identifier" | "type_identifier" | "crate" | "self" | "super" => {
                        segments.push(self.leaf_text(path)?);
                        break;
                    }
                    "scoped_identifier" | "scoped_type_identifier" => current = path,
                    _ => return None,
                },
            }
        }
        segments.reverse();
        Some(segments)
    }

    /// All identifier names a pattern node binds.
    ///
    /// Destructuring contributes every bound identifier. A typed
    /// tuple-struct path (`Some(x)`) also contributes its path
    /// identifier, over-suppressing where the file alone cannot decide.
    fn pattern_names(&self, pattern: Node) -> Vec<&'a str> {
        let mut names = Vec::new();
        if let Some(name) = self.leaf_text(pattern) {
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
                    .and_then(|n| self.leaf_text(n))
            };
            return (kind, name);
        }
        (ItemKind::Other, None)
    }

    /// Text of a path leaf node (identifier, type identifier, or the
    /// `crate`/`self`/`super` keywords), or `None` for any other kind.
    fn leaf_text(&self, node: Node) -> Option<&'a str> {
        if matches!(
            node.kind(),
            "identifier" | "type_identifier" | "crate" | "self" | "super"
        ) {
            node.utf8_text(self.bytes).ok()
        } else {
            None
        }
    }
}

/// Hint at each eligible path, respecting aliases and conditional compilation.
///
/// Macro, attribute, import, ambiguous, and shadowed paths are exempt.
/// Conditional attributes exempt guarded items and the whole containing function.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut walker = Walker {
        bytes: parsed.source.as_bytes(),
        roots: ROOT_SEGMENTS.iter().copied().collect(),
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    let root = parsed.syntax_tree().root_node();
    // Root resolution needs every `use` path in the file up front: an
    // import anywhere in the file roots paths above it.
    walker.collect_roots(root);
    walker.walk(root);
    walker.diagnostics
}

/// Search a function once before emitting any hints, including nested bodies.
fn contains_conditional(node: Node, bytes: &[u8]) -> bool {
    if is_conditional_attribute(node, bytes) {
        return true;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| contains_conditional(child, bytes))
}

/// True when `frame` binds `short` at `occurrence_start`.
///
/// Counts an import of that name, plus bindings in scope there:
/// items anywhere, a `let` from its position onward.
fn frame_mentions(frame: &ScopeFrame<'_>, short: &str, occurrence_start: usize) -> bool {
    frame.imports.iter().any(|import| import.short == short)
        || frame.bindings.iter().any(|binding| {
            binding.name == short && (binding.start == 0 || binding.start <= occurrence_start)
        })
}

/// True when `frame` binds `short` to something other than
/// `segments`: a local binding, or an import of a different path
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

/// Outer attributes are sibling nodes in the pinned Rust grammar.
fn has_conditional_attribute(node: Node, bytes: &[u8]) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(attribute) = previous {
        if is_conditional_attribute(attribute, bytes) {
            return true;
        }
        if !matches!(
            attribute.kind(),
            "attribute_item" | "line_comment" | "block_comment"
        ) {
            break;
        }
        previous = attribute.prev_named_sibling();
    }
    false
}

/// True when `node` is the outermost node of a scoped path chain, so
/// it carries the whole path. Inner chain links are covered by their
/// parent.
fn is_chain_head(node: Node) -> bool {
    matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier")
        && !node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "scoped_identifier" | "scoped_type_identifier"
            )
        })
}

/// Node kinds that introduce a lexical scope: blocks, module and
/// trait bodies, functions with their parameters, closures, loops, and
/// match arms.
fn is_scope(kind: &str) -> bool {
    matches!(
        kind,
        "source_file"
            | "block"
            | "declaration_list"
            | "function_item"
            | "closure_expression"
            | "for_expression"
            | "match_arm"
    )
}

/// The `use` import that hoists `path`: the whole path, or its prefix
/// through the first uppercase-initial segment.
///
/// By Rust naming that segment names a type or trait. Every segment
/// after it is then an associated item, and `use` cannot import one:
/// rustc rejects `use std::sync::Arc::new;` (E0432).
///
/// This naming heuristic avoids suggesting an associated-item import.
///
/// A path of only lowercase segments names modules plus one importable
/// item and stays whole. Snake-case type names break the naming rule,
/// not this advice.
fn use_path(path: &str) -> &str {
    let mut offset = 0;
    for segment in path.split("::") {
        if segment.starts_with(|c: char| c.is_ascii_uppercase()) {
            return &path[..offset + segment.len()];
        }
        offset += segment.len() + 2;
    }
    path
}

/// Recognize real conditional attributes, never their text in comments or strings.
fn is_conditional_attribute(node: Node, bytes: &[u8]) -> bool {
    matches!(node.kind(), "attribute_item" | "inner_attribute_item")
        && node
            .named_child(0)
            .and_then(|attr| attr.named_child(0))
            .is_some_and(|path| {
                path.kind() == "identifier"
                    && path
                        .utf8_text(bytes)
                        .is_ok_and(|text| matches!(text, "cfg" | "cfg_attr"))
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;
    use rstest::rstest;

    /// A first occurrence supplies complete import and replacement advice.
    #[rstest]
    #[case::missing(
        "fn f() { std::sync::Arc::new(1); }",
        "- Add `use std::sync::Arc;` at module scope.\n- Replace this path with `Arc::new`."
    )]
    #[case::imported(
        "use std::sync::Arc; fn f() { std::sync::Arc::new(1); }",
        "- Replace this path with `Arc::new`; `Arc` is already imported."
    )]
    #[case::aliased(
        "use std::sync::Arc as Shared; fn f() { std::sync::Arc::new(1); }",
        "- Replace this path with `Shared::new`; `Shared` is already imported."
    )]
    fn check_should_explain_first_occurrence(#[case] source: &str, #[case] advice: &str) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Hint);
        assert_eq!(
            diagnostics[0].message,
            format!(
                "fully-qualified path `std::sync::Arc::new` makes code harder to read.\n{advice}"
            )
        );
    }

    /// Custom roots need no existing import; type-relative paths remain exempt.
    #[rstest]
    #[case::custom_type("fn f() { vendor::net::Client::new(); }", 1)]
    #[case::custom_function("fn f() { vendor::connect(); }", 1)]
    #[case::associated("fn f() { Client::new(); }", 0)]
    #[case::shadowed("struct Client; fn f() { vendor::net::Client::new(); }", 0)]
    #[case::later_shadow("fn f() { vendor::net::Client::new(); } struct Client;", 0)]
    #[case::primitive("fn f() { str::to_string(\"a\"); }", 0)]
    fn check_should_distinguish_custom_roots(#[case] source: &str, #[case] expected: usize) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), expected);
    }

    /// Conditional compilation exempts the containing function, not its sibling.
    #[rstest]
    #[case::body("fn f() { std::mem::drop(1); #[cfg(unix)] let x = 1; }")]
    #[case::nested("fn f() { std::mem::drop(1); { #[cfg(unix)] let x = 1; } }")]
    #[case::function("#[cfg(unix)] fn f() { std::mem::drop(1); }")]
    #[case::test_configuration("#[cfg(test)] fn f() { std::mem::drop(1); }")]
    #[case::conditional_attribute("#[cfg_attr(unix, allow(unused))] fn f() { std::mem::drop(1); }")]
    #[case::body_cfg_attr(
        "fn f() { std::mem::drop(1); #[cfg_attr(unix, allow(unused))] let x = 1; }"
    )]
    #[case::module("#[cfg(test)] mod tests { fn f() { std::mem::drop(1); } }")]
    #[case::inner_module("mod tests { #![cfg(test)] fn f() { std::mem::drop(1); } }")]
    #[case::implementation("#[cfg(unix)] impl X { fn f() { std::mem::drop(1); } }")]
    #[case::comment_between("#[cfg(unix)] // guard\nfn f() { std::mem::drop(1); }")]
    fn check_should_exempt_conditional_function(#[case] guarded: &str) {
        let source = format!("{guarded}\nfn sibling() {{ std::mem::drop(2); }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].item_name.as_deref(), Some("sibling"));
    }

    /// Directive-like text is not a conditional attribute.
    #[rstest]
    #[case::comment("// #[cfg(unix)]\n")]
    #[case::string("let text = \"#[cfg(test)]\";")]
    #[case::raw_string("let text = r#\"#[cfg_attr(test, ignore)]\"#;")]
    fn check_should_ignore_directive_text(#[case] text: &str) {
        let source = format!("fn f() {{ {text} std::mem::drop(1); }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    /// A cfg-guarded import reserves its name: advising another import
    /// would duplicate it under the active cfg.
    #[test]
    fn check_should_not_advise_import_when_guarded_import_exists() {
        let diagnostics =
            lint("#[cfg(unix)] use std::sync::Arc;\nfn f() { std::sync::Arc::new(1); }");

        assert!(diagnostics.is_empty());
    }

    /// Run MOD003 over a retained parse of `source`.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = parse_source(source).unwrap();
        check(&parsed)
    }

    // ── Import available ──

    // Plain import + qualified occurrence in type position -> one Hint
    // naming the short name at the occurrence's line. It carries the
    // enclosing item's kind and name.
    #[test]
    fn fires_when_fully_qualified_path_is_already_imported() {
        let diags = lint(
            "use std::collections::HashMap;\n\
             fn f() -> std::collections::HashMap {\n\
             Default::default()\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_QUALIFIED_PATH);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("f"));
        assert!(diags[0].message.contains("`std::collections::HashMap`"));
        assert!(diags[0].message.contains("`HashMap` is already imported"));
    }

    // Associated members use the imported type, never an associated-item import.
    #[test]
    fn fires_for_chains_extending_an_imported_prefix() {
        let diags = lint(
            "use std::sync::Arc;\n\
             fn f() {\n\
             let _ = std::sync::Arc::new(1);\n\
             let _ = std::sync::Arc::new(2);\n\
             let _ = std::sync::Arc::new(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("`Arc` is already imported"))
        );
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("Replace this path with `Arc::new`"))
        );
        assert!(
            diags
                .iter()
                .all(|d| !d.message.contains("use std::sync::Arc::new;"))
        );
    }

    // `self` inside a group binds the group's prefix path: `use
    // a::b::{self}` imports `a::b` as `b`.
    #[test]
    fn self_in_a_group_binds_the_group_prefix() {
        let diags = lint(
            "use a::b::{self};\n\
             fn f() {\n\
             let _ = a::b;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`b` is already imported"));
    }

    // Every group member is its own import -> both occurrences fire.
    #[test]
    fn fires_for_each_member_of_a_grouped_import() {
        let diags = lint(
            "use a::b::{C, D};\n\
             fn f() {\n\
             let _ = (a::b::C, a::b::D);\n\
             }\n",
        );

        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("`C`"));
        assert!(diags[1].message.contains("`D`"));
    }

    // Aliased import -> the hint suggests the alias, not the path's
    // last segment.
    #[test]
    fn fires_with_the_alias_when_import_renames() {
        let diags = lint(
            "use a::b::E as F;\n\
             fn f() {\n\
             let _ = a::b::E;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`a::b::E`"));
        assert!(diags[0].message.contains("`F` is already imported"));
    }

    // A top-level import stays in scope inside nested modules.
    #[test]
    fn fires_in_nested_modules_under_a_top_level_import() {
        let diags = lint(
            "use a::b::C;\n\
             mod tests {\n\
             fn t() {\n\
             let _ = a::b::C;\n\
             }\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`C` is already imported"));
    }

    // An out-of-scope import does not replace missing-import advice.
    #[test]
    fn check_should_suggest_import_when_existing_import_is_out_of_scope() {
        let diags = lint(
            "mod inner {\n\
             pub use a::b::C;\n\
             }\n\
             fn f() {\n\
             a::b::C;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Add `use a::b::C;`"));
    }

    // A `use` after the occurrence still selects imported-name advice:
    // items are visible throughout their scope, and in-order advice
    // would duplicate the existing import.
    #[test]
    fn check_should_apply_import_advice_when_use_follows_the_occurrence() {
        let diags = lint("fn f() { std::sync::Arc::new(1); }\nuse std::sync::Arc;");

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Arc` is already imported"));
        assert!(!diags[0].message.contains("Add `use"));
    }

    // ── Missing imports ──

    // Every occurrence names its path and the suggested import.
    #[test]
    fn check_should_hint_at_every_occurrence() {
        let diags = lint(
            "fn f() {\n\
             std::mem::drop(1);\n\
             std::mem::drop(2);\n\
             std::mem::drop(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("f"));
        assert!(diags[0].message.contains("`std::mem::drop`"));
        assert!(diags[0].message.contains("Replace this path with `drop`"));
        assert!(diags[0].message.contains("use std::mem::drop;"));
    }

    // A path ending in an associated item hoists through its type:
    // rustc rejects `use std::sync::Arc::new;` (E0432), and
    // `use std::sync::Arc;` imports the type.
    #[test]
    fn repeated_associated_item_chain_advises_the_importable_prefix() {
        let diags = lint(
            "fn f() {\n\
             std::sync::Arc::new(1);\n\
             std::sync::Arc::new(2);\n\
             std::sync::Arc::new(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert!(diags[0].message.contains("`std::sync::Arc::new`"));
        assert!(diags[0].message.contains("use std::sync::Arc;"));
        assert!(!diags[0].message.contains("use std::sync::Arc::new;"));
    }

    // Existing imports affect advice, not the number of hints.
    #[test]
    fn check_should_hint_at_every_imported_occurrence() {
        let diags = lint(
            "use std::mem::drop;\n\
             fn f() {\n\
             std::mem::drop(1);\n\
             std::mem::drop(2);\n\
             std::mem::drop(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("`drop` is already imported"))
        );
        assert!(diags.iter().all(|d| !d.message.contains("times")));
    }

    // ── Exemptions (contract D7) ──

    // Macro spans include the macro's own path, even when imported.
    #[test]
    fn silent_inside_macro_invocation_and_definition_spans() {
        let diags = lint(
            "use log::debug;\n\
             macro_rules! probe {\n\
             () => {\n\
             std::old::Thing::x();\n\
             };\n\
             }\n\
             fn f() {\n\
             log::debug!(\"a\");\n\
             log::debug!(\"b\");\n\
             log::debug!(\"c\");\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // Attribute spans never count, including the attribute's own scoped path.
    #[test]
    fn silent_inside_attribute_spans() {
        let diags = lint(
            "#[std::old::marker::Probe]\n\
             fn a() {}\n\
             #[std::old::marker::Probe]\n\
             fn b() {}\n\
             #[std::old::marker::Probe]\n\
             fn c() {}\n",
        );

        assert!(diags.is_empty());
    }

    // `use` declaration text itself is never an occurrence.
    #[test]
    fn use_declaration_text_is_never_flagged() {
        let diags = lint(
            "use std::collections::HashMap;\n\
             use a::b::{C, D};\n\
             use a::b::E as F;\n",
        );

        assert!(diags.is_empty());
    }

    // A local binding or parameter shadowing the short name suppresses advice.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_local_binding() {
        let diags = lint(
            "use a::b::C;\n\
             fn f(C: u8) {\n\
             let _ = a::b::C;\n\
             }\n\
             fn g() {\n\
             let C = 1;\n\
             let _ = a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A same-named item in the file scope shadows the import.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_same_named_item() {
        let diags = lint(
            "use a::b::C;\n\
             struct C;\n\
             fn f() {\n\
             let _ = a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A top-level function also binds its name in the file scope, so a
    // suggested `use` of the same short name would conflict.
    #[test]
    fn silent_when_a_top_level_function_shares_the_short_name() {
        let diags = lint(
            "fn probe() {}\n\
             fn f() {\n\
             let _ = crate::x::probe;\n\
             let _ = crate::x::probe;\n\
             let _ = crate::x::probe;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // The shadowing gate checks the advised import's short name
    // (`Arc`), not the repeated path's last segment (`new`): the
    // import is what would conflict.
    #[test]
    fn silent_when_a_top_level_item_shares_the_advised_import_name() {
        let diags = lint(
            "struct Arc;\n\
             fn f() {\n\
             std::sync::Arc::new(1);\n\
             std::sync::Arc::new(2);\n\
             std::sync::Arc::new(3);\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A `let` shadows only from its position: the occurrence before it
    // still fires, the one after stays silent.
    #[test]
    fn fires_before_and_suppresses_after_a_shadowing_let() {
        let diags = lint(
            "use a::b::C;\n\
             fn g() {\n\
             let _ = a::b::C;\n\
             let C = 1;\n\
             let _ = a::b::C;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        assert!(diags[0].message.contains("`C` is already imported"));
    }

    // An if-let pattern binds its name for the rest of the block, so
    // the occurrence after it stays silent.
    #[test]
    fn silent_when_short_name_is_shadowed_by_an_if_let_pattern() {
        let diags = lint(
            "use a::b::C;\n\
             fn g() {\n\
             if let C = 1 {}\n\
             let _ = a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A destructuring `let` binds every pattern identifier.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_destructuring_let() {
        let diags = lint(
            "use a::b::C;\n\
             fn g() {\n\
             let (d, C) = (1, 2);\n\
             let _ = a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A loop pattern binds inside the loop body.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_loop_pattern() {
        let diags = lint(
            "use a::b::C;\n\
             fn g() {\n\
             for C in 0..1 {\n\
             let _ = a::b::C;\n\
             }\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A closure parameter binds inside the closure body.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_closure_parameter() {
        let diags = lint(
            "use a::b::C;\n\
             fn g() {\n\
             let h = |C| a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A match arm pattern binds inside the arm's value.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_match_arm_pattern() {
        let diags = lint(
            "use a::b::C;\n\
             fn g() {\n\
             match 1 {\n\
             C => a::b::C,\n\
             _ => 2,\n\
             };\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // Shadowed imports never fall back to missing-import advice.
    #[test]
    fn check_should_suppress_advice_when_covering_import_is_shadowed() {
        let diags = lint(
            "use std::sync::Arc;\n\
             fn f(Arc: u8) {\n\
             let _ = std::sync::Arc::new(1);\n\
             let _ = std::sync::Arc::new(2);\n\
             let _ = std::sync::Arc::new(3);\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A one-segment import never covers: its suggestion would repeat
    // the path verbatim (`use std;` over `std::mem`).
    #[test]
    fn one_segment_imports_never_cover_a_path() {
        let diags = lint(
            "use std;\n\
             fn f() {\n\
             std::mem::drop(1);\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Add `use std::mem::drop;`"));
    }

    // Two imports binding one short name make the advice ambiguous:
    // no replacement advice is safe.
    #[test]
    fn silent_when_short_name_is_ambiguous_across_imports() {
        let diags = lint(
            "use a::X;\n\
             use b::X;\n\
             fn f() {\n\
             let _ = a::X;\n\
             let _ = a::X;\n\
             let _ = a::X;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // ── Root resolution and globs ──

    // Glob imports provide root evidence, not a known imported name.
    #[test]
    fn check_should_suggest_explicit_import_when_only_glob_exists() {
        let diags = lint(
            "use a::b::*;\n\
             fn f() {\n\
             let _ = a::b::C;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Add `use a::b::C;`"));
    }

    // Hints follow occurrence order across different paths.
    #[test]
    fn check_should_order_hints_by_occurrence() {
        let diags = lint(
            "fn f() {\n\
             std::mem::drop(1);\n\
             std::mem::drop(2);\n\
             std::mem::drop(3);\n\
             std::io::copy(&mut a, &mut b);\n\
             std::io::copy(&mut a, &mut b);\n\
             std::io::copy(&mut a, &mut b);\n\
             }\n",
        );

        assert_eq!(diags.len(), 6);
        assert!(diags[0].message.contains("`std::mem::drop`"));
        assert!(diags[3].message.contains("`std::io::copy`"));
        assert!(diags[0].line < diags[1].line);
    }

    // Same-line hints follow source order, not alphabetical path order.
    #[test]
    fn check_should_order_same_line_hints_by_occurrence() {
        let diags = lint(
            "fn f(mut a: u8, mut b: u8) {\n\
             let _ = (std::mem::drop(&a), std::io::copy(&mut a, &mut b));\n\
             let _ = (std::mem::drop(&a), std::io::copy(&mut a, &mut b));\n\
             let _ = (std::mem::drop(&a), std::io::copy(&mut a, &mut b));\n\
             }\n",
        );

        assert_eq!(diags.len(), 6);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[1].line, 2);
        assert!(diags[0].message.contains("`std::mem::drop`"));
        assert!(diags[1].message.contains("`std::io::copy`"));
    }

    // Multi-segment paths without a rooted first segment (enum variant
    // paths) never count.
    #[test]
    fn silent_for_paths_without_a_rooted_first_segment() {
        let diags = lint(
            "enum Error { X }\n\
             fn f() {\n\
             let _ = Error::X;\n\
             let _ = Error::X;\n\
             let _ = Error::X;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }
}
