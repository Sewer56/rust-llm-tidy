//! Intra-file reference-edge collection via `syn::visit::Visit`.
//!
//! [`ReferenceCollector`] walks a parsed AST and records `(item,
//! referenced_item)` edges for every bare path reference whose first segment
//! matches a known top-level item name. Edges to local macros are reversed
//! so a macro definition precedes its use sites.

use std::collections::HashSet;
use syn::visit::Visit;
use syn::{
    ItemConst, ItemEnum, ItemFn, ItemMacro, ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUnion,
};

/// Collects intra-file reference edges by walking the AST with `syn::visit::Visit`.
///
/// Tracks which top-level item we are currently inside (`item_stack`) and records
/// `(item, referenced_item)` edges for every bare path reference whose first
/// segment matches a known top-level item name.
pub struct ReferenceCollector {
    /// Stack of current top-level item names we are inside.
    item_stack: Vec<String>,
    /// Set of all top-level item names (all kinds).
    top_level_names: HashSet<String>,
    /// Set of top-level macro names; edges to macros are reversed so the
    /// macro definition precedes its use sites.
    macro_names: HashSet<String>,
    /// Edges: (item_name, referenced_item).
    edges: Vec<(String, String)>,
}

impl ReferenceCollector {
    /// Create a new collector seeded with all top-level item names.
    pub fn new(top_level_names: HashSet<String>, macro_names: HashSet<String>) -> Self {
        Self {
            item_stack: Vec::new(),
            top_level_names,
            macro_names,
            edges: Vec::new(),
        }
    }

    /// Walk `file` and record reference edges for later retrieval via [`into_edges`](Self::into_edges).
    pub fn collect(&mut self, file: &syn::File) {
        self.visit_file(file);
    }

    /// Consume the collector and return discovered reference edges.
    pub fn into_edges(self) -> Vec<(String, String)> {
        self.edges
    }

    /// Scan a macro body's token stream for references to top-level items.
    ///
    /// Macro bodies (`macro_rules!` / macro 2.0) are raw `TokenStream`s that
    /// `syn` does not parse into AST nodes, so the default `Visit` traversal
    /// never fires `visit_path` inside them. This recovers those references by
    /// scanning tokens directly. An `ident` matching a top-level name records a
    /// `(current, name)` edge; if the ident is followed by `!` (a macro call)
    /// and the target is a local macro, the edge is reversed to
    /// `(name, current)` so the referenced macro definition precedes its use.
    /// `Group` delimiters (parentheses, braces, brackets) are recursed into.
    fn collect_refs_from_tokens(&mut self, tokens: &proc_macro2::TokenStream, current: &str) {
        use proc_macro2::TokenTree;
        let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
        for (i, tree) in trees.iter().enumerate() {
            match tree {
                TokenTree::Ident(ident) => {
                    let name = ident.to_string();
                    if name.as_str() != current && self.top_level_names.contains(&name) {
                        let is_macro_call = matches!(
                            trees.get(i + 1),
                            Some(TokenTree::Punct(p)) if p.as_char() == '!'
                        );
                        if is_macro_call && self.macro_names.contains(&name) {
                            self.edges.push((name, current.to_string()));
                        } else {
                            self.edges.push((current.to_string(), name));
                        }
                    }
                }
                TokenTree::Group(g) => {
                    self.collect_refs_from_tokens(&g.stream(), current);
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for ReferenceCollector {
    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        let name = f.sig.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_fn(self, f);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_fn(self, f);
        }
    }

    fn visit_item_struct(&mut self, s: &'ast ItemStruct) {
        let name = s.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_struct(self, s);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_struct(self, s);
        }
    }

    fn visit_item_enum(&mut self, e: &'ast ItemEnum) {
        let name = e.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_enum(self, e);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_enum(self, e);
        }
    }

    fn visit_item_union(&mut self, u: &'ast ItemUnion) {
        let name = u.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_union(self, u);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_union(self, u);
        }
    }

    fn visit_item_type(&mut self, t: &'ast ItemType) {
        let name = t.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_type(self, t);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_type(self, t);
        }
    }

    fn visit_item_const(&mut self, c: &'ast ItemConst) {
        let name = c.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_const(self, c);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_const(self, c);
        }
    }

    fn visit_item_static(&mut self, s: &'ast ItemStatic) {
        let name = s.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_static(self, s);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_static(self, s);
        }
    }

    fn visit_item_trait(&mut self, t: &'ast ItemTrait) {
        let name = t.ident.to_string();
        if self.top_level_names.contains(&name) {
            self.item_stack.push(name);
            syn::visit::visit_item_trait(self, t);
            self.item_stack.pop();
        } else {
            syn::visit::visit_item_trait(self, t);
        }
    }

    fn visit_item_macro(&mut self, m: &'ast ItemMacro) {
        // `m.ident` is `Some` only for named macro definitions (macro_rules!,
        // macro 2.0); `None` for invocations like `foo!()`.
        if let Some(ident) = &m.ident {
            let name = ident.to_string();
            if self.top_level_names.contains(&name) {
                // Macro bodies are raw TokenStreams that syn does not parse
                // into AST nodes, so the default visitor never fires visit_path
                // on references inside them (e.g. `macro_rules! foo` calling
                // `bar!()` in its body). Scan the token stream directly.
                self.collect_refs_from_tokens(&m.mac.tokens, &name);
                return;
            }
        }
        syn::visit::visit_item_macro(self, m);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(current) = self.item_stack.last()
            && let Some(first_seg) = path.segments.first()
        {
            let name = first_seg.ident.to_string();
            if self.top_level_names.contains(&name) && name != *current {
                if self.macro_names.contains(&name) {
                    // Macro definitions must precede their uses, so reverse
                    // the edge: (macro, consumer) instead of (consumer, macro).
                    self.edges.push((name, current.clone()));
                } else {
                    self.edges.push((current.clone(), name));
                }
            }
        }
        syn::visit::visit_path(self, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Macro references are inverted so the macro definition precedes its use.
    #[test]
    fn test_reference_collector_macro_edge_reversed() {
        let source = r#"
            fn b() { a!(); }
            macro_rules! a { () => {}; }
        "#;

        let top_level_names: HashSet<String> =
            ["b".to_string(), "a".to_string()].into_iter().collect();
        let macro_names: HashSet<String> = ["a".to_string()].into_iter().collect();

        let file: syn::File = syn::parse_str(source).unwrap();
        let mut collector = ReferenceCollector::new(top_level_names, macro_names);
        collector.visit_file(&file);
        let edges = collector.into_edges();

        assert_eq!(edges, vec![("a".to_string(), "b".to_string())]);
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

        let top_level_names: HashSet<String> =
            ["a".to_string(), "b".to_string(), "Foo".to_string()]
                .into_iter()
                .collect();

        let file: syn::File = syn::parse_str(source).unwrap();
        let mut collector = ReferenceCollector::new(top_level_names, HashSet::new());
        collector.visit_file(&file);
        let edges = collector.into_edges();

        // fn a references Foo (type), fn b references a (fn)
        assert_eq!(edges.len(), 2);
        assert!(edges.contains(&("a".to_string(), "Foo".to_string())));
        assert!(edges.contains(&("b".to_string(), "a".to_string())));
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

        let top_level_names: HashSet<String> =
            ["A".to_string(), "B".to_string()].into_iter().collect();

        let file: syn::File = syn::parse_str(source).unwrap();
        let mut collector = ReferenceCollector::new(top_level_names, HashSet::new());
        collector.visit_file(&file);
        let edges = collector.into_edges();

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0], ("B".to_string(), "A".to_string()));
    }
}
