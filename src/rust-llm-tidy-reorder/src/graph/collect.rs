//! Intra-file reference-edge collection via `syn::visit::Visit`.
//!
//! [`ReferenceCollector`] walks a parsed AST and records `(item_index,
//! referenced_item_index)` edges for every bare path reference whose first
//! segment matches a known top-level item name. Edges to local macros are
//! reversed so a macro definition precedes its use sites.
//!
//! # Allocation strategy
//!
//! Identifiers are probed against the name map by writing each one into a
//! single reused scratch [`String`] (via its `fmt::Write` impl), so the hot
//! `visit_path` / token-scan paths perform zero per-ident heap allocation.
//! Edges are stored as item indices, not owned strings.

use ahash::{AHashMap, AHashSet};
use std::fmt::Write as _;
use syn::visit::Visit;
use syn::{
    ItemConst, ItemEnum, ItemFn, ItemMacro, ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUnion,
};

/// Collects intra-file reference edges by walking the AST with `syn::visit::Visit`.
///
/// Tracks which top-level item we are currently inside (`item_stack`, by index)
/// and records `(referencer_index, referenced_index)` edges for every bare path
/// reference whose first segment matches a known top-level item name.
///
/// The `name_to_idx` map and `macro_names` set borrow `&str` slices from the
/// parsed items (lifetime `'names`); they are only queried, never mutated.
pub struct ReferenceCollector<'names> {
    /// Stack of current top-level item *indices* we are inside.
    item_stack: Vec<usize>,
    /// Top-level item name -> item index, borrowed from the parse.
    name_to_idx: AHashMap<&'names str, usize>,
    /// Set of top-level macro names; edges to macros are reversed so the
    /// macro definition precedes its use sites.
    macro_names: AHashSet<&'names str>,
    /// Edges: `(referencer_index, referenced_index)`.
    edges: Vec<(usize, usize)>,
    /// Reused buffer for ident -> `&str` conversion during probing. Writing an
    /// ident via `fmt::Write` fills existing capacity instead of allocating,
    /// so the hot Visit paths never heap-allocate per ident.
    scratch: String,
}

impl<'names> ReferenceCollector<'names> {
    /// Create a new collector seeded with a name-to-index map and the macro
    /// name set. Both borrow `&str` slices that must outlive the collector
    /// (typically the name fields of the parsed items).
    pub fn new(
        name_to_idx: AHashMap<&'names str, usize>,
        macro_names: AHashSet<&'names str>,
    ) -> Self {
        Self {
            item_stack: Vec::new(),
            name_to_idx,
            macro_names,
            edges: Vec::new(),
            scratch: String::new(),
        }
    }

    /// Walk `file` and record reference edges for later retrieval via [`into_edges`](Self::into_edges).
    pub fn collect(&mut self, file: &syn::File) {
        self.visit_file(file);
    }

    /// Consume the collector and return discovered reference edges as
    /// `(referencer_index, referenced_index)` pairs.
    pub fn into_edges(self) -> Vec<(usize, usize)> {
        self.edges
    }

    /// Write `ident` into the scratch buffer and return its item index if it
    /// names a top-level item. One reusable buffer, no per-ident allocation.
    fn probe(&mut self, ident: &proc_macro2::Ident) -> Option<usize> {
        self.scratch.clear();
        let _ = write!(self.scratch, "{ident}");
        self.name_to_idx.get(self.scratch.as_str()).copied()
    }

    /// Scan a macro body's token stream for references to top-level items.
    ///
    /// Macro bodies (`macro_rules!` / macro 2.0) are raw `TokenStream`s that
    /// `syn` does not parse into AST nodes, so the default `Visit` traversal
    /// never fires `visit_path` inside them. This recovers those references by
    /// scanning tokens directly. An `ident` naming a top-level item records a
    /// `(current, target)` edge; if the ident is followed by `!` (a macro call)
    /// and the target is a local macro, the edge is reversed to
    /// `(target, current)` so the referenced macro definition precedes its use.
    /// `Group` delimiters (parentheses, braces, brackets) are recursed into.
    ///
    /// Iteration uses a `peekable` iterator over the (cloned) stream so the
    /// `!` lookahead needs no materialized `Vec<TokenTree>`.
    fn collect_refs_from_tokens(&mut self, tokens: &proc_macro2::TokenStream, current_idx: usize) {
        use proc_macro2::TokenTree;
        let mut iter = tokens.clone().into_iter().peekable();
        while let Some(tree) = iter.next() {
            match &tree {
                TokenTree::Ident(ident) => {
                    self.scratch.clear();
                    let _ = write!(self.scratch, "{ident}");
                    let name = self.scratch.as_str();
                    if let Some(&target_idx) = self.name_to_idx.get(name)
                        && target_idx != current_idx
                    {
                        let is_macro_call = matches!(
                            iter.peek(),
                            Some(TokenTree::Punct(p)) if p.as_char() == '!'
                        );
                        if is_macro_call && self.macro_names.contains(name) {
                            self.edges.push((target_idx, current_idx));
                        } else {
                            self.edges.push((current_idx, target_idx));
                        }
                    }
                }
                TokenTree::Group(g) => {
                    self.collect_refs_from_tokens(&g.stream(), current_idx);
                }
                _ => {}
            }
        }
    }
}

impl<'ast, 'names> Visit<'ast> for ReferenceCollector<'names> {
    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if let Some(idx) = self.probe(&f.sig.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_fn(self, f);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_struct(&mut self, s: &'ast ItemStruct) {
        if let Some(idx) = self.probe(&s.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_struct(self, s);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_struct(self, s);
        }
    }

    fn visit_item_enum(&mut self, e: &'ast ItemEnum) {
        if let Some(idx) = self.probe(&e.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_enum(self, e);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_enum(self, e);
        }
    }

    fn visit_item_union(&mut self, u: &'ast ItemUnion) {
        if let Some(idx) = self.probe(&u.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_union(self, u);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_union(self, u);
        }
    }

    fn visit_item_type(&mut self, t: &'ast ItemType) {
        if let Some(idx) = self.probe(&t.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_type(self, t);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_type(self, t);
        }
    }

    fn visit_item_const(&mut self, c: &'ast ItemConst) {
        if let Some(idx) = self.probe(&c.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_const(self, c);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_const(self, c);
        }
    }

    fn visit_item_static(&mut self, s: &'ast ItemStatic) {
        if let Some(idx) = self.probe(&s.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_static(self, s);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_static(self, s);
        }
    }

    fn visit_item_trait(&mut self, t: &'ast ItemTrait) {
        if let Some(idx) = self.probe(&t.ident) {
            self.item_stack.push(idx);
            syn::visit::visit_item_trait(self, t);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_trait(self, t);
        }
    }

    fn visit_item_macro(&mut self, m: &'ast ItemMacro) {
        // `m.ident` is `Some` only for named macro definitions (macro_rules!,
        // macro 2.0); `None` for invocations like `foo!()`.
        if let Some(ident) = &m.ident
            && let Some(idx) = self.probe(ident)
        {
            // Macro bodies are raw TokenStreams that syn does not parse
            // into AST nodes, so the default visitor never fires visit_path
            // on references inside them (e.g. `macro_rules! foo` calling
            // `bar!()` in its body). Scan the token stream directly.
            self.collect_refs_from_tokens(&m.mac.tokens, idx);
            return;
        }
        syn::visit::visit_item_macro(self, m);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(&current_idx) = self.item_stack.last()
            && let Some(first_seg) = path.segments.first()
        {
            self.scratch.clear();
            let _ = write!(self.scratch, "{}", first_seg.ident);
            let name = self.scratch.as_str();
            if let Some(&target_idx) = self.name_to_idx.get(name)
                && target_idx != current_idx
            {
                if self.macro_names.contains(name) {
                    // Macro definitions must precede their uses, so reverse
                    // the edge: (macro, consumer) instead of (consumer, macro).
                    self.edges.push((target_idx, current_idx));
                } else {
                    self.edges.push((current_idx, target_idx));
                }
            }
        }
        syn::visit::visit_path(self, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let name_to_idx = idx_map(&["b", "a"]);
        let macro_names: AHashSet<&str> = ["a"].into_iter().collect();

        let file: syn::File = syn::parse_str(source).unwrap();
        let mut collector = ReferenceCollector::new(name_to_idx, macro_names);
        collector.visit_file(&file);
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

        let file: syn::File = syn::parse_str(source).unwrap();
        let mut collector = ReferenceCollector::new(name_to_idx, AHashSet::new());
        collector.visit_file(&file);
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

        let file: syn::File = syn::parse_str(source).unwrap();
        let mut collector = ReferenceCollector::new(name_to_idx, AHashSet::new());
        collector.visit_file(&file);
        let edges = collector.into_edges();

        // B(1) references A(0).
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0], (1, 0));
    }
}
