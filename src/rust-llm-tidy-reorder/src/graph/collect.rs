//! Intra-file reference-edge collection via a tree-sitter tree walk.
//!
//! [`ReferenceCollector`] walks a parsed syntax tree and records
//! `(item_index, referenced_item_index)` edges for every reference whose
//! first segment matches a known top-level item name.
//!
//! All node-kind matching - which nodes declare items, which reference
//! positions record a use, which identifier spots define names - comes
//! from a [`ReferenceWalk`] supplied by the language's reorder profile.
//!
//! The same walk serves any grammar's tree.
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
//! item stack; kinds the data omits are not pushed, so references inside
//! them are ignored.
//!
//! Within a pushed item, every reference position the walk data declares -
//! a bare identifier, a path shape, a wrapped type, a macro call - whose
//! first segment names a top-level item records an edge.
//!
//! A recorded path immediately followed by the walk's call marker counts as
//! a macro call: an edge to a locally defined macro reverses, so the
//! definition precedes its use.

use super::profile::{ReferencePosition, ReferenceWalk};
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

    /// Recursive tree walk. Pushes declared item indices, records
    /// reference edges at the walk's reference positions, and recurses
    /// into compound nodes.
    fn walk(&mut self, node: Node, source: &[u8]) {
        let kind = node.kind();

        // Pushed item kinds come from the walk data: determine index by
        // name, push, recurse, pop.
        if self.walk.declaration_kinds.contains(&kind) {
            let pushed = self
                .name_index_of_decl(node, source)
                .inspect(|&idx| self.item_stack.push(idx));
            self.recurse(node, source);
            if pushed.is_some() {
                self.item_stack.pop();
            }
            return;
        }

        let Some(position) = position_of(self.walk, kind) else {
            // Pure structure - blocks, expressions, parameters, field
            // lists, type arguments, ...: interior references surface on
            // recursion.
            //
            // (When the item stack is empty - e.g. inside an impl body
            // at top level - `record_ref` records nothing.)
            self.recurse(node, source);
            return;
        };

        match position.path_field {
            // The referenced path is a field's child: a macro call
            // records its called path once and never walks it again
            // (re-walking it would double-record); the argument token
            // tree is not scanned either.
            Some(field) => {
                if let Some(path) = node.child_by_field_name(field) {
                    self.record_ref(path, source);
                }
            }
            // The node itself is the path. A bare identifier also names
            // declarations, and declaration names are not uses; a path
            // shape is a use by construction.
            None => {
                let bare = position.segment_field.is_none();
                if !bare || !is_decl_position(self.walk, node) {
                    self.record_ref(node, source);
                }
            }
        }

        // Wrapped shapes keep walking: their children hold further
        // references (the type arguments of a generic type).
        if position.recurse {
            self.recurse(node, source);
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

    /// True when `node` (a recorded path) is immediately followed by the
    /// walk's call marker, i.e. it is a macro call path.
    fn is_macro_call(&self, node: Node) -> bool {
        node.next_sibling()
            .is_some_and(|sibling| sibling.kind() == self.walk.macro_marker_kind)
    }

    /// Write the first segment's text of `node` into the scratch buffer and
    /// return its top-level item index, if any. Leaves `self.scratch` holding
    /// the segment text on success.
    fn probe_first_segment(&mut self, node: Node, source: &[u8]) -> Option<usize> {
        let seg = first_segment_node(self.walk, node)?;
        self.scratch.clear();
        // Copy the segment text (borrowed from `source`) into the reusable
        // scratch buffer, then probe by `&str`. No per-ident allocation.
        if let Ok(text) = seg.utf8_text(source) {
            self.scratch.push_str(text);
        }
        self.name_to_idx.get(self.scratch.as_str()).copied()
    }
}

/// Leftmost segment node of a recorded path node, resolved through the
/// walk's segment fields until a bare identifier kind.
fn first_segment_node<'t>(walk: &'static ReferenceWalk, node: Node<'t>) -> Option<Node<'t>> {
    let position = position_of(walk, node.kind())?;
    match position.segment_field {
        Some(field) => node
            .child_by_field_name(field)
            .and_then(|child| first_segment_node(walk, child)),
        // A wrapper without a segment field never resolves as its own
        // path: the walk records its path field's child instead.
        None if position.path_field.is_none() => Some(node),
        None => None,
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

/// The reference-position entry for `kind`, if the walk data declares one.
fn position_of(walk: &'static ReferenceWalk, kind: &str) -> Option<&'static ReferencePosition> {
    walk.reference_positions
        .iter()
        .find(|position| position.kind == kind)
}
