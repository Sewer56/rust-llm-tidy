//! Crate-level Rust reference facts: module paths, module files, and
//! reference edges from one whole-crate parse.
//!
//! Two constructors:
//!
//! - [`RustCrateIndex::from_parsed`]: the pure core. It reuses
//!   [`build_module_paths`] for module resolution, then walks each
//!   parsed tree once to collect reference edges.
//! - [`RustCrateIndex::build`]: the discovery wrapper, mirroring
//!   `resolve_vis_context` (`src/pipeline/files/vis.rs`).
//!
//! Blind spots: re-export veils, dyn and trait dispatch, paths inside
//! macro token trees, and single-segment `use` paths (`use a::{..}`
//! stays silent; scoped prefixes resolve).
//!
//! Multi-crate inputs index one crate: the one owning the first `.rs`
//! input.

use crate::input;
use crate::rules::transform::visibility::rust::{
    ModulePaths, ParsedFile, build_module_paths, discover_crate_root,
};
use ahash::AHashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Crate-level facts from one whole-crate parse.
///
/// Holds each parsed file's module-path segments, the collected
/// reference edges, and non-fatal module-resolution warnings.
pub(crate) struct RustCrateIndex {
    /// Resolved file -> module-path segments (crate root -> file).
    paths: ModulePaths,
    /// Reference edges, in file discovery order then source order.
    edges: Vec<ReferenceEdge>,
}

/// One resolved module reference: the referencing file, the resolved
/// target module's file, and the 1-based line of the reference.
///
/// A `crate::` path resolving to the file's own module yields
/// `from == target`.
pub(crate) struct ReferenceEdge {
    /// Path of the file containing the reference, in the form passed
    /// to the constructor (`build` canonicalizes to absolute).
    pub(crate) from: PathBuf,
    /// Path of the resolved target module's file (same form as `from`).
    pub(crate) target: PathBuf,
    /// 1-based line of the referencing path expression.
    pub(crate) line: usize,
}

impl RustCrateIndex {
    /// Build the index from pre-parsed files (pure core, no discovery).
    ///
    /// Module paths come from [`build_module_paths`]; only files inside
    /// that resolved tree contribute edges.
    ///
    /// # Arguments
    ///
    /// - `root` - the source file to treat as the crate root.
    /// - `files` - every parsed file in the crate, in discovery order;
    ///   edge order follows this order, then source order per file.
    ///
    /// # Returns
    ///
    /// `Ok` with the built index; never `Err` (see `# Errors`).
    ///
    /// # Errors
    ///
    /// Always `Ok(RustCrateIndex)`; the `anyhow::Result` return mirrors
    /// [`build_module_paths`] for API continuity.
    pub(crate) fn from_parsed(root: &Path, files: &[ParsedFile]) -> anyhow::Result<Self> {
        let paths = build_module_paths(root, files)?;

        // Reverse map (module path -> file), built in discovery order so
        // duplicate module paths resolve to the first parsed file.
        let mut file_of: AHashMap<String, PathBuf> = AHashMap::new();
        let mut key = String::new();
        for pf in files {
            let Some(segs) = paths.segments_for(&pf.path) else {
                continue;
            };
            key.clear();
            for (i, seg) in segs.iter().enumerate() {
                if i > 0 {
                    key.push_str("::");
                }
                key.push_str(seg);
            }
            file_of
                .entry(key.clone())
                .or_insert_with(|| pf.path.clone());
        }

        let mut edges = Vec::new();
        for pf in files {
            if let Some(current) = paths.segments_for(&pf.path) {
                collect_edges(pf, current, &file_of, &mut edges);
            }
        }

        Ok(RustCrateIndex { paths, edges })
    }

    /// Build the index from the first `.rs` input's owning crate,
    /// mirroring `resolve_vis_context` (`src/pipeline/files/vis.rs`)
    /// in the `vis` step.
    ///
    /// Discovers the crate root, canonicalizes it, parses every `.rs`
    /// under the crate directory once, then calls [`Self::from_parsed`].
    ///
    /// # Arguments
    ///
    /// - `inputs` - the run's input paths; the first `.rs` selects the
    ///   crate.
    /// - `warnings` - sink for the one failure warning, per-file parse
    ///   warnings (skipped files), and module-resolution warnings;
    ///   unreadable files skip silently, and a failed directory walk
    ///   still indexes the files it collected.
    ///
    /// # Returns
    ///
    /// `Some(index)` when discovery succeeds; `None` (with one warning
    /// pushed) on discovery failure, and `None` without a warning when
    /// there is no `.rs` input.
    pub(crate) fn build(inputs: &[PathBuf], warnings: &mut Vec<String>) -> Option<Self> {
        let first = inputs
            .iter()
            .find(|p| input::ext_in(p.extension().and_then(|e| e.to_str()), &["rs"]))?;
        // Canonicalize the crate root so it matches the canonicalized
        // source paths collected below (symlinked temp dirs otherwise
        // break the root lookup).
        let root = match discover_crate_root(first) {
            Ok(r) => fs::canonicalize(&r).unwrap_or(r),
            Err(e) => {
                warnings.push(format!("rust crate index unavailable ({e})"));
                return None;
            }
        };

        // Collect every .rs under the crate src dir, parse each once.
        let crate_dir = root.parent().unwrap_or_else(|| Path::new("."));
        let mut rs_files: Vec<PathBuf> = Vec::new();
        let _ = input::collect_files(crate_dir, &["rs"], &mut rs_files, true);
        let mut files: Vec<ParsedFile> = Vec::with_capacity(rs_files.len());
        for f in &rs_files {
            if let Ok(src) = fs::read_to_string(f) {
                let path = fs::canonicalize(f).unwrap_or_else(|_| f.clone());
                match ParsedFile::new(path, src) {
                    Ok(pf) => files.push(pf),
                    Err(e) => warnings.push(format!("could not parse {}: {e}", f.display())),
                }
            }
        }

        let index = match Self::from_parsed(&root, &files) {
            Ok(i) => i,
            Err(e) => {
                warnings.push(format!("failed to build rust crate index ({e:?})"));
                return None;
            }
        };
        warnings.extend(index.warnings().iter().cloned());
        Some(index)
    }

    /// Module-path segments for `file`, root first.
    ///
    /// # Arguments
    ///
    /// - `file` - the file's path in the form passed to the
    ///   constructor (`build` canonicalizes to absolute).
    ///
    /// # Returns
    ///
    /// The module-path segments from the crate root to `file`; empty at
    /// the crate root; `None` for files outside the resolved tree.
    pub(crate) fn segments_for(&self, file: &Path) -> Option<&[Box<str>]> {
        self.paths.segments_for(file)
    }

    /// The collected reference edges.
    ///
    /// # Returns
    ///
    /// Edges in file discovery order, then source order within a file.
    pub(crate) fn edges(&self) -> &[ReferenceEdge] {
        &self.edges
    }

    /// Non-fatal module-resolution warnings.
    ///
    /// # Returns
    ///
    /// The collected warnings in discovery order; empty when every
    /// `mod` resolved.
    pub(crate) fn warnings(&self) -> &[String] {
        self.paths.warnings()
    }
}

/// Walk one file's tree once, appending resolved edges to `out`.
///
/// `gate_depth` counts enclosing `#[cfg(test)]`-gated items on one
/// reused cursor; the walk skips chain-head path nodes inside a gate.
fn collect_edges(
    pf: &ParsedFile,
    current: &[Box<str>],
    file_of: &AHashMap<String, PathBuf>,
    out: &mut Vec<ReferenceEdge>,
) {
    let source = pf.source.as_str();
    let root = pf.tree.root_node();
    let mut cursor = root.walk();
    let mut gate_depth = 0usize;
    'walk: loop {
        let node = cursor.node();
        if is_test_gated(node, source) {
            gate_depth += 1;
        } else if gate_depth == 0
            && is_chain_head(node)
            && let Some(target) = node.utf8_text(source.as_bytes()).ok().and_then(|text| {
                resolve_target(&text.split("::").collect::<Vec<_>>(), current, file_of)
            })
        {
            out.push(ReferenceEdge {
                from: pf.path.clone(),
                target,
                line: node.start_position().row + 1,
            });
        }
        if cursor.goto_first_child() {
            continue 'walk;
        }
        // Leave the current node exactly once: each climb drops the
        // gate contribution of the node the cursor exits.
        loop {
            let leaving = cursor.node();
            if is_test_gated(leaving, source) {
                gate_depth -= 1;
            }
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == root {
                return;
            }
        }
    }
}

/// True when `node` is the outermost link of a scoped path chain, so its
/// text carries the whole path (the parent covers inner links).
fn is_chain_head(node: Node) -> bool {
    matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier")
        && !node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "scoped_identifier" | "scoped_type_identifier"
            )
        })
}

/// True when `node` is a gate-kind item preceded by a `#[cfg(test)]`
/// attribute run (attributes are preceding siblings in the grammar).
fn is_test_gated(node: Node, source: &str) -> bool {
    is_gate_kind(node.kind()) && has_cfg_test_attribute(node, source)
}

/// Resolve one path's segments against the referencing module path.
///
/// Resolution rules:
///
/// - A leading `crate` restarts at the root.
/// - Each leading `super` drops one segment from `current`; overflow
///   skips the path. A leading `self` stays put.
/// - Otherwise the path is bare and must resolve under `current`.
///
/// The target is the longest prefix present in `file_of` that is
/// longer than the base. Two cases therefore contribute nothing:
///
/// - A path staying inside the referencing module.
/// - An unmatched path: an extern crate or a path matching no module
///   prefix.
fn resolve_target(
    segments: &[&str],
    current: &[Box<str>],
    file_of: &AHashMap<String, PathBuf>,
) -> Option<PathBuf> {
    let first = segments.first().copied()?;
    let (base_len, rest): (usize, &[&str]) = if first == "crate" {
        (0, &segments[1..])
    } else if first == "super" || first == "self" {
        // Unwind one module level per `super`; `self` stays put.
        let mut depth = current.len();
        let mut i = 0;
        while i < segments.len() && (segments[i] == "super" || segments[i] == "self") {
            if segments[i] == "super" {
                if depth == 0 {
                    return None; // `super` overflow past the crate root
                }
                depth -= 1;
            }
            i += 1;
        }
        (depth, &segments[i..])
    } else {
        (current.len(), segments)
    };

    // Full candidate path: the base module segments plus the rest.
    let mut full: Vec<&str> = Vec::with_capacity(base_len + rest.len());
    full.extend(current[..base_len].iter().map(|s| &**s));
    full.extend(rest.iter().copied());
    if full.len() <= base_len {
        return None;
    }

    // Longest joined prefix present in file_of, reused-key lookup.
    let mut key = String::new();
    let mut target = None;
    for (i, seg) in full.iter().enumerate() {
        if i > 0 {
            key.push_str("::");
        }
        key.push_str(seg);
        if i + 1 > base_len
            && let Some(path) = file_of.get(&key)
        {
            target = Some(path.clone());
        }
    }
    target
}

/// Scan `node`'s contiguous preceding `attribute_item` run for
/// `#[cfg(test)]`; comments do not break the run.
///
/// The attribute text must equal `cfg(test)` after trimming, so
/// `cfg_attr` and other predicates never match.
fn has_cfg_test_attribute(node: Node, source: &str) -> bool {
    let mut prev = node.prev_named_sibling();
    while let Some(item) = prev {
        match item.kind() {
            "attribute_item" => {
                let gated = (0..item.named_child_count() as u32).any(|i| {
                    item.named_child(i).is_some_and(|attr| {
                        attr.kind() == "attribute" && attr_text_is_cfg_test(attr, source)
                    })
                });
                if gated {
                    return true;
                }
            }
            // Comments are trivia and do not break the attribute run.
            "line_comment" | "block_comment" => {}
            _ => return false,
        }
        prev = item.prev_named_sibling();
    }
    false
}

/// Item kinds whose subtree a preceding `#[cfg(test)]` gate covers.
fn is_gate_kind(kind: &str) -> bool {
    matches!(
        kind,
        "mod_item"
            | "use_declaration"
            | "function_item"
            | "static_item"
            | "const_item"
            | "struct_item"
            | "enum_item"
            | "impl_item"
    )
}

/// True when `attr`'s trimmed text is exactly `cfg(test)`.
fn attr_text_is_cfg_test(attr: Node, source: &str) -> bool {
    attr.utf8_text(source.as_bytes())
        .is_ok_and(|t| t.trim() == "cfg(test)")
}

#[cfg(test)]
mod tests {
    use super::{ParsedFile, RustCrateIndex};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn src(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    /// Parse `(path, source)` pairs into [`ParsedFile`]s (discovery
    /// order = slice order).
    fn parse_files(sources: Vec<(PathBuf, String)>) -> Vec<ParsedFile> {
        sources
            .into_iter()
            .map(|(p, s)| ParsedFile::new(p, s).expect("test source must parse"))
            .collect()
    }

    /// Build the index over the synthetic crate rooted at `src/lib.rs`.
    fn index(sources: Vec<(PathBuf, String)>) -> RustCrateIndex {
        let files = parse_files(sources);
        RustCrateIndex::from_parsed(&src("src/lib.rs"), &files).expect("index builds")
    }

    /// The `(from, target, line)` triples of an index's edges.
    fn edge_triples(idx: &RustCrateIndex) -> Vec<(&Path, &Path, usize)> {
        idx.edges()
            .iter()
            .map(|e| (e.from.as_path(), e.target.as_path(), e.line))
            .collect()
    }

    // Construction: module-path mapping.

    #[test]
    fn index_should_expose_module_segments_per_file() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod a;\n".into()),
            (src("src/a/mod.rs"), "mod b;\n".into()),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);
        let segs: Vec<String> = idx
            .segments_for(&src("src/a/b.rs"))
            .expect("b.rs in tree")
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(segs, vec!["a", "b"]);
        assert_eq!(
            idx.segments_for(&src("src/lib.rs")),
            Some([].as_slice()),
            "crate root has no segments"
        );
    }

    // Core behavior: reference edges across path forms.

    #[test]
    fn index_should_record_load_to_xbe_edge_when_load_calls_crate_path() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                src("src/load/mod.rs"),
                "pub fn go() {\n    crate::xbe::parse_xbe_header();\n}\n".into(),
            ),
            (
                src("src/xbe.rs"),
                "pub fn parse_xbe_header() {}\npub struct Header;\n".into(),
            ),
        ]);
        assert_eq!(
            edge_triples(&idx),
            vec![(Path::new("src/load/mod.rs"), Path::new("src/xbe.rs"), 2)],
            "one load -> xbe edge at the call's line"
        );
        assert!(idx.segments_for(&src("src/load/mod.rs")).is_some());
        assert!(idx.segments_for(&src("src/xbe.rs")).is_some());
    }

    #[test]
    fn index_should_record_signature_only_edge_when_return_type_uses_crate_path() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                src("src/load/mod.rs"),
                "pub fn make() -> crate::xbe::Header {\n    todo!()\n}\n".into(),
            ),
            (src("src/xbe.rs"), "pub struct Header;\n".into()),
        ]);
        assert_eq!(
            edge_triples(&idx),
            vec![(Path::new("src/load/mod.rs"), Path::new("src/xbe.rs"), 1)],
            "type-position paths count like any other reference"
        );
    }

    #[test]
    fn index_should_record_root_edge_when_root_uses_bare_module_path() {
        let idx = index(vec![
            (
                src("src/lib.rs"),
                "use load::XbeFormat;\nmod load;\nmod xbe;\n".into(),
            ),
            (src("src/load/mod.rs"), "pub struct XbeFormat;\n".into()),
            (src("src/xbe.rs"), "pub fn f() {}\n".into()),
        ]);
        assert_eq!(
            edge_triples(&idx),
            vec![(Path::new("src/lib.rs"), Path::new("src/load/mod.rs"), 1)],
            "bare multi-segment path resolves via its first module segment"
        );
    }

    #[test]
    fn index_should_resolve_super_path_when_nested_module_references_sibling() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod a;\n".into()),
            (src("src/a/mod.rs"), "mod inner;\nmod b;\n".into()),
            (
                src("src/a/inner.rs"),
                "pub fn t() { super::b::f(); }\n".into(),
            ),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);
        assert_eq!(
            edge_triples(&idx),
            vec![(Path::new("src/a/inner.rs"), Path::new("src/a/b.rs"), 1)],
            "super unwinds to the parent module before resolving"
        );
    }

    #[test]
    fn index_should_pick_deepest_module_when_nested_prefixes_both_match() {
        let idx = index(vec![
            (
                src("src/lib.rs"),
                "mod a;\npub fn go() { crate::a::b::f(); }\n".into(),
            ),
            (src("src/a/mod.rs"), "mod b;\npub fn g() {}\n".into()),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);
        // Both `a` and `a::b` match the path `crate::a::b::f`; the
        // deepest module prefix must win.
        assert_eq!(
            edge_triples(&idx),
            vec![(Path::new("src/lib.rs"), Path::new("src/a/b.rs"), 2)],
            "deepest matching module prefix wins"
        );
    }

    // Edge cases: silent paths and gated regions.

    #[test]
    fn index_should_stay_silent_when_paths_match_no_module() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod load;\n".into()),
            (
                src("src/load/mod.rs"),
                "use crate::std::HashMap;\nuse std::collections::BTreeMap;\nfn g<T>() { g::<u8>(); Vec::<u8>::new(); }\n".into(),
            ),
        ]);
        assert!(
            idx.edges().is_empty(),
            "extern crates, unmatched paths, and turbofish contribute nothing: {:?}",
            edge_triples(&idx)
        );
    }

    #[test]
    fn index_should_skip_references_when_inside_cfg_test_gate() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                src("src/load/mod.rs"),
                "#[cfg(test)]\n// unit tests below\nmod tests {\n    use crate::xbe::Header;\n}\n"
                    .into(),
            ),
            (src("src/xbe.rs"), "pub struct Header;\n".into()),
        ]);
        assert!(
            idx.edges().is_empty(),
            "cfg(test)-gated subtrees are not production callers"
        );
    }

    #[test]
    fn index_should_skip_stray_file_when_outside_module_tree() {
        let idx = index(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (src("src/load/mod.rs"), "pub fn go() {}\n".into()),
            (src("src/xbe.rs"), "pub fn f() {}\n".into()),
            // Stray file: never declared by any `mod`.
            (
                src("src/stray.rs"),
                "pub fn s() { crate::xbe::f(); }\n".into(),
            ),
        ]);
        assert!(
            idx.edges().is_empty(),
            "files outside the module tree contribute no edges"
        );
        assert!(
            idx.segments_for(&src("src/stray.rs")).is_none(),
            "stray file resolves to no module path"
        );
    }

    // Discovery wrapper contract (no crate metadata in tests).

    #[test]
    fn build_should_return_none_without_warning_when_no_rs_input() {
        let mut warnings = Vec::new();
        let idx = RustCrateIndex::build(&[PathBuf::from("docs/README.md")], &mut warnings);
        assert!(idx.is_none(), "no .rs input selects no crate");
        assert!(warnings.is_empty(), "missing .rs input is not a warning");
    }

    /// `build` discovers the crate through its `Cargo.toml`, so the
    /// happy path needs a real (dependency-free) manifest fixture.
    #[test]
    fn build_should_index_crate_when_manifest_resolves() {
        let dir =
            std::env::temp_dir().join(format!("rust-llm-tidy-crate-build-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src/load")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(dir.join("src/lib.rs"), "mod load;\nmod xbe;\n").unwrap();
        fs::write(
            dir.join("src/load/mod.rs"),
            "pub fn go() { crate::xbe::f(); }\n",
        )
        .unwrap();
        fs::write(dir.join("src/xbe.rs"), "pub fn f() {}\n").unwrap();

        let mut warnings = Vec::new();
        let idx = RustCrateIndex::build(&[dir.join("src/lib.rs")], &mut warnings);

        let idx = idx.unwrap_or_else(|| panic!("index builds; warnings: {warnings:?}"));
        assert_eq!(idx.edges().len(), 1);
        let edge = &idx.edges()[0];
        assert!(
            edge.from.ends_with("load/mod.rs"),
            "{}",
            edge.from.display()
        );
        assert!(
            edge.target.ends_with("src/xbe.rs"),
            "{}",
            edge.target.display()
        );
        assert_eq!(edge.line, 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
