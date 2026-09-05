//! Intra-file reference-edge collection via a tree-sitter tree walk.
//!
//! [`ReferenceCollector`] walks a parsed syntax tree and records
//! `(item_index, referenced_item_index)` edges for every bare path reference
//! whose first segment matches a known top-level item name.
//!
//! The node-kind matching comes from a [`ReferenceWalk`] supplied by the
//! language's reorder profile, so the same walk serves any grammar's tree.
//!
//! Edges to local macros are reversed so a macro definition precedes its use
//! sites.
//!
//! # Allocation strategy
//!
//! Identifiers are probed against the name map by writing each one into a
//! single reused scratch [`String`] (via its `fmt::Write` impl), so the hot
//! reference paths perform zero per-ident heap allocation.
//!
//! Edges are stored as item indices, not owned strings.
//!
//! # Walk model
//!
//! Only NAMED top-level items the walk data declares push an index onto the
//! item stack (for the Rust grammar: functions, structs, enums, unions,
//! type aliases, consts, statics, traits, and `macro_rules!` definitions).
//!
//! Impls, modules, uses, extern crates, and macro invocations are NOT
//! pushed, so references inside them are ignored - mirroring the prior
//! `syn::visit::Visit` behavior.
//!
//! Within a pushed item, every reference position (a path/type identifier or
//! a scoped identifier) whose first segment names a top-level item records an
//! edge.
//!
//! Macro calls (`ident!`) to a local macro record a reversed edge so the
//! definition precedes its use.

use super::profile::ReferenceWalk;
use ahash::{AHashMap, AHashSet};
use tree_sitter::{Node, Tree};

/// Collects intra-file reference edges by walking a tree-sitter tree.
///
/// Tracks which top-level item we are currently inside (`item_stack`, by index)
/// and records `(referencer_index, referenced_index)` edges for every reference
/// whose first segment matches a known top-level item name.
///
/// The `name_to_idx` map and `macro_names` set borrow `&str` slices from the
/// parsed items (lifetime `'names`); they are only queried, never mutated.
/// `walk` supplies the grammar's node-kind data.
pub struct ReferenceCollector<'names> {
    /// Stack of current top-level item *indices* we are inside.
    item_stack: Vec<usize>,
    /// Top-level item name -> item index, borrowed from the parse.
    name_to_idx: AHashMap<&'names str, usize>,
    /// Set of top-level macro names; edges to macros are reversed so the
    /// macro definition precedes its use sites.
    macro_names: AHashSet<&'names str>,
    /// The grammar's node-kind data, from the language's reorder profile.
    walk: &'static ReferenceWalk,
    /// Edges: `(referencer_index, referenced_index)`.
    edges: Vec<(usize, usize)>,
    /// Reused buffer for ident -> `&str` conversion during probing. Writing an
    /// ident via `fmt::Write` fills existing capacity instead of allocating,
    /// so the hot walk paths never heap-allocate per ident.
    scratch: String,
}

impl<'names> ReferenceCollector<'names> {
    /// Create a new collector seeded with a name-to-index map, the macro
    /// name set, and the grammar's walk data. The map and set borrow `&str`
    /// slices that must outlive the collector (typically the name fields of
    /// the parsed items).
    pub fn new(
        name_to_idx: AHashMap<&'names str, usize>,
        macro_names: AHashSet<&'names str>,
        walk: &'static ReferenceWalk,
    ) -> Self {
        Self {
            item_stack: Vec::new(),
            name_to_idx,
            macro_names,
            walk,
            edges: Vec::new(),
            scratch: String::new(),
        }
    }

    /// Walk `tree` and record reference edges for later retrieval via [`into_edges`].
    /// `source` is the full source text, used to extract identifier text.
    ///
    /// [`into_edges`]: ReferenceCollector::into_edges
    pub fn collect(&mut self, tree: &Tree, source: &[u8]) {
        self.walk(tree.root_node(), source);
    }

    /// Consume the collector and return discovered reference edges as
    /// `(referencer_index, referenced_index)` pairs.
    pub fn into_edges(self) -> Vec<(usize, usize)> {
        self.edges
    }

    /// Recursive tree walk. Pushes named item indices, records reference edges
    /// for path/type identifiers, and recurses into compound nodes.
    fn walk(&mut self, node: Node, source: &[u8]) {
        // Pushed item kinds come from the walk data: determine index by
        // name, push, recurse, pop.
        if self.walk.declaration_kinds.contains(&node.kind()) {
            let pushed = self
                .name_index_of_decl(node, source)
                .inspect(|&idx| self.item_stack.push(idx));
            self.recurse(node, source);
            if pushed.is_some() {
                self.item_stack.pop();
            }
            return;
        }

        match node.kind() {
            // `macro_invocation` is a reference site (its `macro` field is a
            // macro-call path). Record it once and do NOT recurse: the macro
            // path identifier is recorded here, and re-walking it would
            // double-record.
            //
            // The invocation's argument token_tree is not scanned (mirrors
            // syn, which only scanned macro *definition* bodies, not
            // invocation arguments).
            "macro_invocation" => {
                if let Some(mac) = node.child_by_field_name("macro") {
                    self.record_ref(mac, source);
                }
            }

            // A scoped identifier or scoped type identifier is a single path:
            // record its FIRST segment only, do not recurse into segments.
            "scoped_identifier" | "scoped_type_identifier" => {
                self.record_ref(node, source);
            }

            // A generic type (`Vec<Foo>`): record the outer type's first
            // segment, then recurse into `type_arguments` to catch inner type
            // references (e.g. `Foo` in `Vec<Foo>`).
            "generic_type" => {
                if let Some(ty) = node.child_by_field_name("type") {
                    self.record_ref(ty, source);
                }
                self.recurse(node, source);
            }

            // A bare identifier / type_identifier in a reference position.
            "identifier" | "type_identifier" => {
                if !is_decl_position(self.walk, node) {
                    self.record_ref(node, source);
                }
            }

            // Non-pushed, non-reference nodes: recurse into children. This
            // covers blocks, expressions, parameters, field lists, type
            // arguments, etc. - their interior references are found on
            // recursion.
            //
            // (When the item stack is empty - e.g. inside an impl body at
            // top level - `record_ref` records nothing.)
            _ => {
                self.recurse(node, source);
            }
        }
    }

    /// Recurse into the named children of `node`.
    fn recurse(&mut self, node: Node, source: &[u8]) {
        let count = node.named_child_count() as u32;
        for i in 0..count {
            if let Some(child) = node.named_child(i) {
                self.walk(child, source);
            }
        }
    }

    /// Look up the top-level item index for a declaration node's name, if any.
    fn name_index_of_decl(&mut self, node: Node, source: &[u8]) -> Option<usize> {
        let name_node = node.child_by_field_name("name")?;
        self.probe_first_segment(name_node, source)
    }

    /// Record a reference edge from the current item to the item named by the
    /// first segment of `node` (a path/type identifier), if it names a
    /// top-level item other than the current one.
    ///
    /// Macro calls to a local macro reverse the edge so the definition
    /// precedes its use.
    fn record_ref(&mut self, node: Node, source: &[u8]) {
        let Some(&current_idx) = self.item_stack.last() else {
            // Not inside a named top-level item: no edge (mirrors syn, which
            // only recorded edges when item_stack was non-empty).
            return;
        };
        let is_macro_call = self.is_macro_call(node);
        // `probe_first_segment` writes the first segment into `self.scratch`
        // and returns the target index (copied), so no borrow is held across
        // the edge push. `self.scratch` still holds the name afterwards.
        let Some(target_idx) = self.probe_first_segment(node, source) else {
            return;
        };
        if target_idx == current_idx {
            return;
        }
        let is_macro_target = is_macro_call && self.macro_names.contains(self.scratch.as_str());
        if is_macro_target {
            // Macro definitions must precede their uses, so reverse the edge.
            self.edges.push((target_idx, current_idx));
        } else {
            self.edges.push((current_idx, target_idx));
        }
    }

    /// True when `node` (an identifier/scoped identifier) is immediately
    /// followed by `!`, i.e. it is a macro call path.
    fn is_macro_call(&self, node: Node) -> bool {
        node.next_sibling().is_some_and(|s| s.kind() == "!")
    }

    /// Write the first segment's text of `node` into the scratch buffer and
    /// return its top-level item index, if any. Leaves `self.scratch` holding
    /// the segment text on success.
    fn probe_first_segment(&mut self, node: Node, source: &[u8]) -> Option<usize> {
        let seg = first_segment_node(node)?;
        if !matches!(seg.kind(), "identifier" | "type_identifier") {
            return None;
        }
        self.scratch.clear();
        // Copy the segment text (borrowed from `source`) into the reusable
        // scratch buffer, then probe by `&str`. No per-ident allocation.
        if let Ok(text) = seg.utf8_text(source) {
            self.scratch.push_str(text);
        }
        self.name_to_idx.get(self.scratch.as_str()).copied()
    }
}

/// Leftmost segment node of a path/type node.
fn first_segment_node(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" | "type_identifier" => Some(node),
        "scoped_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("path")
            .and_then(first_segment_node),
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(first_segment_node),
        _ => None,
    }
}

/// True when `node` (an `identifier`/`type_identifier`) is in a declaration
/// position (an item name, a binding pattern, an alias) rather than a
/// reference position. Declaration names are not recorded as references;
/// the spots come from the walk data.
fn is_decl_position(walk: &'static ReferenceWalk, node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let Some(field) = parent_field_name(node) else {
        return false;
    };
    let parent_kind = parent.kind();
    walk.decl_name_positions
        .iter()
        .any(|pos| parent_kind == pos.parent_kind && field == pos.field)
}

/// Field name of `node` within its parent, if any.
fn parent_field_name(node: Node) -> Option<&'static str> {
    let parent = node.parent()?;
    // Find the child index of `node` among the parent's children (named +
    // anonymous) and look up its field name.
    let count = parent.child_count() as u32;
    for i in 0..count {
        if parent.child(i) == Some(node) {
            return parent.field_name_for_child(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{DeclNamePosition, ReorderProfile, RustProfile};
    use rust_llm_tidy_lang::{LanguageBackend, RustBackend};

    /// Build a name-to-index map assigning each name a position index in the
    /// order given (decoupled from source item order, so unit tests stay stable).
    fn idx_map(names: &[&'static str]) -> AHashMap<&'static str, usize> {
        names.iter().enumerate().map(|(i, &n)| (n, i)).collect()
    }

    /// Macro references are inverted so the macro definition precedes its use.
    #[test]
    fn test_reference_collector_macro_edge_reversed() {
        let source = r#"
            fn b() { a!(); }
            macro_rules! a { () => {}; }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let name_to_idx = idx_map(&["b", "a"]);
        let macro_names: AHashSet<&str> = ["a"].into_iter().collect();

        let tree = parsed.syntax_tree();
        let mut collector =
            ReferenceCollector::new(name_to_idx, macro_names, RustProfile.reference_walk());
        collector.collect(tree, source.as_bytes());
        let edges = collector.into_edges();

        // b(0) calls macro a(1); reversed edge (a=1, b=0).
        assert_eq!(edges, vec![(1, 0)]);
    }

    /// `ReferenceCollector` produces edges for fn-to-fn and fn-to-type references.
    #[test]
    fn test_reference_collector_finds_fn_and_type_refs() {
        let source = r#"
            use std::collections::HashMap;

            struct Foo {
                x: i32,
            }

            impl Foo {
                fn new() -> Self {
                    Foo { x: 0 }
                }
            }

            fn a() {
                let f = Foo::new();
            }

            fn b() {
                a();
            }
        "#;

        // Indices: a=0, b=1, Foo=2 (listed order, decoupled from source).
        let name_to_idx = idx_map(&["a", "b", "Foo"]);

        let parsed = RustBackend.parse(source).unwrap();
        let tree = parsed.syntax_tree();
        let mut collector =
            ReferenceCollector::new(name_to_idx, AHashSet::new(), RustProfile.reference_walk());
        collector.collect(tree, source.as_bytes());
        let edges = collector.into_edges();

        // fn a(0) references Foo(2) (type), fn b(1) references a(0) (fn)
        assert_eq!(edges.len(), 2);
        assert!(edges.contains(&(0, 2)));
        assert!(edges.contains(&(1, 0)));
    }

    /// `ReferenceCollector` records edges for struct-to-struct references.
    #[test]
    fn test_cross_type_dependency() {
        let source = r#"
            struct A {}

            struct B {
                a: A,
            }
        "#;

        // Indices: A=0, B=1 (listed order).
        let name_to_idx = idx_map(&["A", "B"]);

        let parsed = RustBackend.parse(source).unwrap();
        let tree = parsed.syntax_tree();
        let mut collector =
            ReferenceCollector::new(name_to_idx, AHashSet::new(), RustProfile.reference_walk());
        collector.collect(tree, source.as_bytes());
        let edges = collector.into_edges();

        // B(1) references A(0).
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0], (1, 0));
    }

    /// Walk data drives declaration matching: a walk declaring `mod_item`
    /// as a declaration kind records references inside a mod body that the
    /// Rust walk (which skips mods) ignores.
    #[test]
    fn walk_data_drives_declaration_matching() {
        let source = "mod m { fn f() { g(); } }\nfn g() {}\n";
        // Indices: m=0, g=1 (listed order).
        let name_to_idx = idx_map(&["m", "g"]);

        static MOD_WALK: ReferenceWalk = ReferenceWalk {
            declaration_kinds: &["mod_item"],
            decl_name_positions: &[DeclNamePosition::new("mod_item", "name")],
        };

        let parsed = RustBackend.parse(source).unwrap();
        let tree = parsed.syntax_tree();

        let mut rust_walk = ReferenceCollector::new(
            name_to_idx.clone(),
            AHashSet::new(),
            RustProfile.reference_walk(),
        );
        rust_walk.collect(tree, source.as_bytes());
        assert!(
            rust_walk.into_edges().is_empty(),
            "the Rust walk never pushes a mod body"
        );

        let mut mod_walk = ReferenceCollector::new(name_to_idx, AHashSet::new(), &MOD_WALK);
        mod_walk.collect(tree, source.as_bytes());
        let edges = mod_walk.into_edges();

        // Inside mod m(0), fn f calls g(1): edge (0, 1).
        assert_eq!(edges, vec![(0, 1)]);
    }
}
