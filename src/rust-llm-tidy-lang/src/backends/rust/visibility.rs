//! Rust-only visibility narrowing: bare `pub` items inside
//! restricted-visibility modules are narrowed to the module's visibility,
//! via cross-file module-tree resolution.
//!
//! A `pub` item inside a `pub(crate)` module is effectively `pub(crate)` -
//! the bare `pub` keyword overstates the item's reachability.
//!
//! [`narrow_vis_in_tree`] rewrites each affected child's `pub` token to the
//! most-restrictive enclosing module's visibility.
//!
//! This is a Rust-only API beside the backend's shared AST ops; it is not
//! part of the [`LanguageBackend`] contract, and no other language has
//! visibility narrowing.
//!
//! [`LanguageBackend`]: crate::backends::LanguageBackend

use super::RustBackend;
use crate::backends::LanguageBackend;
use ahash::AHashSet;
pub use modules::{ModuleTree, build_module_tree, discover_crate_root};
pub use narrow::narrow_vis_in_tree;
use std::path::PathBuf;
use tree_sitter::{Node, Tree};

mod modules;
mod narrow;

/// One parsed source file: its path, source text, and tree-sitter tree.
///
/// Built once (via [`ParsedFile::new`]) and reused by [`build_module_tree`] and
/// [`collect_crate_reexports`], so the crate-wide passes parse each file exactly
/// once instead of re-parsing per pass.
///
/// The [`Tree`] stores byte offsets (not references), so it stays valid for
/// `source`'s bytes as long as they are not mutated.
pub struct ParsedFile {
    /// Canonical file path (matches [`ModuleTree`] keys when crate-aware).
    pub path: PathBuf,
    /// The verbatim source text the tree was parsed from.
    pub source: String,
    /// The tree-sitter syntax tree. `pub(crate)`: the narrowing and
    /// re-export passes access it; external callers use `path`/`source`.
    pub(crate) tree: Tree,
}

/// Crate-wide set of simple names re-exported via `pub use` across every file
/// in a crate, plus the glob sentinel `"*"`.
///
/// Built by [`collect_crate_reexports`]. A glob (`pub use p::*`) in ANY file
/// records `"*"`, which (soundness) disables narrowing for every named child
/// across the crate - matching the per-file glob behavior at a crate scope.
///
/// This is the conservative default for cross-file glob scope; a
/// finer-grained per-module-path glob is left as future work.
#[derive(Default)]
pub struct ReexportSet(AHashSet<String>);

impl ParsedFile {
    /// Parse `source` (from the file at `path`) into a [`ParsedFile`].
    ///
    /// # Errors
    ///
    /// tree-sitter performs error recovery, so syntactically invalid Rust still
    /// yields a tree (possibly with `ERROR` nodes) rather than a parse error;
    /// the `Result` is only `Err` when the parser cannot be allocated or the
    /// language is not set.
    pub fn new(path: PathBuf, source: String) -> anyhow::Result<Self> {
        let tree = parse(&source)?;
        Ok(Self { path, source, tree })
    }
}

impl ReexportSet {
    /// Empty set (no re-exports, no glob).
    pub fn new() -> Self {
        Self::default()
    }

    /// True if `name` is re-exported anywhere in the crate, or if a glob was
    /// seen (the `"*"` sentinel disables narrowing for every named child).
    pub fn blocks(&self, name: &str) -> bool {
        self.0.contains(name) || self.0.contains("*")
    }

    /// True if a `pub use ... ::*` glob was seen in any file.
    pub fn has_glob(&self) -> bool {
        self.0.contains("*")
    }

    /// Read-only access for callers that walk the set directly.
    pub fn names(&self) -> &AHashSet<String> {
        &self.0
    }
}

/// Build a crate-wide [`ReexportSet`] by scanning every parsed file's `pub use`
/// items (top-level and nested in inline modules). Mirrors the shared
/// `collect_reexports` (private, same logic) but unions across all files.
///
/// This is the hard-correctness gate's data source: a missed cross-file
/// re-export turns a safe narrowing into a soundness bug, so the caller MUST
/// pass every `.rs` file in the crate.
///
/// # Arguments
///
/// - `files` - iterated over every parsed file; each file's `pub use` items
///   (top-level and nested in inline modules) are scanned and unioned.
pub fn collect_crate_reexports<'a>(files: impl IntoIterator<Item = &'a ParsedFile>) -> ReexportSet {
    let mut out = ReexportSet::new();
    for pf in files {
        let mut per_file: Option<AHashSet<String>> = None;
        collect_reexports(pf.tree.root_node(), pf.source.as_bytes(), &mut per_file);
        if let Some(names) = per_file {
            out.0.extend(names);
        }
    }
    out
}

/// First named child of `node` whose kind equals `kind`.
#[inline]
pub(crate) fn child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let count = node.named_child_count() as u32;
    (0..count).find_map(|i| {
        let c = node.named_child(i)?;
        (c.kind() == kind).then_some(c)
    })
}

/// The `visibility_modifier` child of `node`, if present (bare `pub`,
/// `pub(crate)`, etc.). `None` for private (inherited-visibility) items.
#[inline]
pub(crate) fn visibility_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let count = node.named_child_count() as u32;
    for i in 0..count {
        let c = node.named_child(i)?;
        if c.kind() == "visibility_modifier" {
            return Some(c);
        }
    }
    None
}

/// Collect the simple names re-exported by any `pub use` (bare-`pub` visibility)
/// found at any depth in `container` - top-level and inside inline modules. A
/// glob (`pub use p::*`) records the sentinel "*".
///
/// The set is allocated lazily via the `Option`: it stays `None` (no
/// allocation) for files with no `pub use`, which is the common case.
fn collect_reexports(container: Node, source: &[u8], out: &mut Option<AHashSet<String>>) {
    let count = container.named_child_count() as u32;
    for i in 0..count {
        let item = container.named_child(i).unwrap();
        match item.kind() {
            "use_declaration" => {
                // Only a bare `pub use` widens reach; `pub(crate) use` does not.
                if visibility_node(item).is_some_and(is_bare_pub)
                    && let Some(arg) = item.child_by_field_name("argument")
                {
                    let set = out.get_or_insert_with(AHashSet::new);
                    collect_use_clause(arg, source, set);
                }
            }
            "mod_item" => {
                if let Some(body) = item.child_by_field_name("body") {
                    collect_reexports(body, source, out);
                }
            }
            _ => {}
        }
    }
}

/// Parse `source` with the Rust backend's grammar. The returned [`Tree`]
/// stores byte offsets (not references), so it stays valid for the caller's
/// source bytes as long as those bytes are not mutated.
fn parse(source: &str) -> anyhow::Result<Tree> {
    // Visibility consumes only nodes, not the backend's shared item model.
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&RustBackend.language()?)?;
    parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter parse returned no tree"))
}

/// Expand a single `use` clause (the `argument` of a `use_declaration`) into
/// its re-exported simple names. Mirrors the prior `syn::UseTree` walk:
///
/// - terminal `identifier`/`type_identifier` -> its text.
/// - `scoped_identifier` (`a::b::c`) -> the last segment (`name` field).
/// - `use_as_clause` (`x as alias`) -> the `alias` field.
/// - `use_wildcard` (`a::*`) -> the sentinel `"*"`.
/// - `scoped_use_list` (`a::{b, c}`) -> recurse the `list`, ignoring the path
///   prefix; a `self` in the list re-exports the path's last segment.
/// - `use_list` (`{b, c}`) -> recurse each child.
fn collect_use_clause(node: Node, source: &[u8], out: &mut AHashSet<String>) {
    match node.kind() {
        "identifier" | "type_identifier" => {
            if let Ok(t) = node.utf8_text(source) {
                out.insert(t.to_string());
            }
        }
        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name")
                && let Ok(t) = name.utf8_text(source)
            {
                out.insert(t.to_string());
            }
        }
        "use_as_clause" => {
            if let Some(alias) = node.child_by_field_name("alias")
                && let Ok(t) = alias.utf8_text(source)
            {
                out.insert(t.to_string());
            }
        }
        "use_wildcard" => {
            out.insert(String::from("*"));
        }
        "scoped_use_list" => {
            // A `self` in the list re-exports the path prefix's last segment.
            let path_last = node
                .child_by_field_name("path")
                .and_then(|p| last_segment_text(p, source))
                .map(str::to_string);
            if let Some(list) = node.child_by_field_name("list") {
                let n = list.named_child_count() as u32;
                for i in 0..n {
                    let child = list.named_child(i).unwrap();
                    if child.kind() == "self" {
                        if let Some(name) = &path_last {
                            out.insert(name.clone());
                        }
                    } else {
                        collect_use_clause(child, source, out);
                    }
                }
            }
        }
        "use_list" => {
            let n = node.named_child_count() as u32;
            for i in 0..n {
                collect_use_clause(node.named_child(i).unwrap(), source, out);
            }
        }
        _ => {}
    }
}

/// True when a `visibility_modifier` node is a bare `pub` (no restriction). The
/// `pub` keyword is an anonymous token, so a bare `pub` has zero named
/// children; `pub(crate)`/`pub(super)`/`pub(in path)` each have one named child.
#[inline]
fn is_bare_pub(vis: Node<'_>) -> bool {
    vis.named_child_count() == 0
}

/// Last path-segment text of a path node (`identifier`/`type_identifier`, or the
/// `name` field of a `scoped_identifier`). `None` for non-path nodes - mirroring
/// syn, which only matched `Type::Path`.
fn last_segment_text<'a>(node: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" | "type_identifier" => node.utf8_text(source).ok(),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ParsedFile, collect_crate_reexports};
    use std::path::PathBuf;

    fn src() -> PathBuf {
        PathBuf::from("test.rs")
    }

    /// Parse `src` into a [`ParsedFile`] (test helper).
    fn parse(text: &str) -> ParsedFile {
        ParsedFile::new(src(), text.to_string()).expect("test source must parse")
    }

    #[test]
    fn crate_reexports_union_across_files() {
        // File A re-exports `f`; file B re-exports `g` (group). Neither alone
        // would block both names; the crate-wide union must block f AND g.
        let a = parse("pub use crate::foo::f;\n");
        let b = parse("pub use crate::foo::{g, h};\n");
        let set = collect_crate_reexports([&a, &b]);
        assert!(set.blocks("f"), "f re-exported in file a");
        assert!(set.blocks("g"), "g re-exported in file b (group)");
        assert!(set.blocks("h"), "h re-exported in file b (group)");
        assert!(!set.blocks("x"), "x is not re-exported anywhere");
    }

    #[test]
    fn crate_reexports_glob_in_any_file_disables_crate_wide() {
        // A glob in ONE file sets the sentinel; every named child is blocked
        // crate-wide (conservative soundness default).
        let a = parse("pub fn untouched() {}\n");
        let b = parse("pub use crate::foo::*;\n");
        let set = collect_crate_reexports([&a, &b]);
        assert!(set.has_glob(), "glob sentinel set by file b");
        assert!(set.blocks("anything"), "glob blocks every name crate-wide");
    }

    #[test]
    fn crate_reexports_rename_uses_alias() {
        // `pub use foo::f as alias;` keys by the alias, matching the per-file guard.
        let a = parse("pub use crate::foo::f as alias;\n");
        let set = collect_crate_reexports([&a]);
        assert!(set.blocks("alias"), "rename keyed by alias");
        assert!(
            !set.blocks("f"),
            "original name is not the re-exported name"
        );
    }

    #[test]
    fn crate_reexports_self_keys_by_path_last_segment() {
        // `pub use a::b::{self};` re-exports module `b` under the name `b`.
        let a = parse("pub use a::b::{self};\n");
        let set = collect_crate_reexports([&a]);
        assert!(set.blocks("b"), "self re-exports the path's last segment");
        assert!(!set.blocks("a"), "path prefix is not re-exported by name");
    }
}
