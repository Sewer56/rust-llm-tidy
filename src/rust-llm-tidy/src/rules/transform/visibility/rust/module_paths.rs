//! Module-path segments per file, for crate-aware visibility narrowing.
//!
//! Maps each reachable source file to the declared `mod` name segments from
//! the crate root, reusing the exact resolution rules of `modules.rs`
//! (`build_module_tree`).
//!
//! Edition `foo.rs` preference, `foo/mod.rs` fallback, and `#[path]`
//! overrides all apply.

use self::modules::{ModChild, mod_children_dir, resolve_mod_children};
use super::ParsedFile;
use ahash::{AHashMap, AHashSet};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Cross-file `mod` resolution shared by [`build_module_paths`] and
/// [`build_module_tree`].
///
/// [`build_module_tree`]: super::build_module_tree
pub(super) mod modules;

/// Each resolved file's module path: the declared `mod` name segments from
/// the crate root to that file.
///
/// The crate root itself maps to an empty sequence.
///
/// Built by [`build_module_paths`] with the same resolution rules as
/// [`build_module_tree`].
///
/// [`build_module_tree`]: super::build_module_tree
pub struct ModulePaths {
    /// Resolved absolute file path -> module-path segments from the
    /// crate root.
    paths: AHashMap<PathBuf, Vec<Box<str>>>,
    /// Files with at least one ungated `mod` chain from the root.
    production: AHashSet<PathBuf>,
    /// Non-fatal resolution warnings, same content as
    /// [`ModuleTree::warnings`].
    ///
    /// [`ModuleTree::warnings`]: super::ModuleTree::warnings
    warnings: Vec<String>,
}

impl ModulePaths {
    /// Module-path segments for `file`, root first.
    ///
    /// # Arguments
    ///
    /// - `file` - the resolved absolute path of the source file to look up.
    ///
    /// # Returns
    ///
    /// The declared `mod` name segments from the crate root to `file`; empty
    /// for the crate root. `None` for files outside the resolved tree.
    pub fn segments_for(&self, file: &Path) -> Option<&[Box<str>]> {
        self.paths.get(file).map(Vec::as_slice)
    }

    /// True if `file` is a known node in this map.
    ///
    /// # Arguments
    ///
    /// - `file` - the resolved absolute path of the source file to test.
    ///
    /// # Returns
    ///
    /// `true` when `file` has an entry in the map.
    pub fn contains(&self, file: &Path) -> bool {
        self.paths.contains_key(file)
    }

    /// Iterate every resolved `(file, segments)` pair.
    ///
    /// # Returns
    ///
    /// An iterator over resolved absolute file paths and their module-path
    /// segments, in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&Path, &[Box<str>])> {
        self.paths
            .iter()
            .map(|(path, segs)| (path.as_path(), segs.as_slice()))
    }

    /// True if `file` is reachable from the crate root only through
    /// `#[cfg(test)]`-gated `mod` chains.
    ///
    /// Gated mods still resolve like any other mod (segments and
    /// warnings). The flag marks files whose references must not count
    /// as production callers.
    ///
    /// # Arguments
    ///
    /// - `file` - the resolved absolute path of the source file to test.
    ///
    /// # Returns
    ///
    /// `true` when `file` is in the tree but no ungated path reaches it.
    pub fn is_test_only(&self, file: &Path) -> bool {
        self.paths.contains_key(file) && !self.production.contains(file)
    }

    /// Non-fatal resolution warnings (unresolved `mod`, missing `#[path]`).
    ///
    /// # Returns
    ///
    /// The collected warnings in discovery order; empty when every `mod`
    /// resolved.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Build the module path of each reachable file, root -> leaf.
///
/// Reuses the exact resolution rules of [`build_module_tree`] (edition
/// `foo.rs` preference, `foo/mod.rs` fallback, `#[path]` overrides), so
/// both walks resolve a file the same way.
///
/// [`build_module_tree`]: super::build_module_tree
///
/// Children enqueue with the parent's segments plus the declared module
/// name. On duplicate `mod` edges to one file the first path inserted
/// wins (the LIFO stack records the last-declared edge first).
///
/// The walk also tracks `#[cfg(test)]`-gated edges; see
/// [`ModulePaths::is_test_only`].
///
/// # Arguments
///
/// - `root` - the source file to treat as the crate root (it maps to an
///   empty segment sequence).
/// - `files` - every parsed file in the crate, indexed by resolved absolute
///   path for resolving `mod foo;` edges.
///
/// # Returns
///
/// `Ok` with the built [`ModulePaths`]; never `Err` (see `# Errors`).
///
/// # Errors
///
/// Always `Ok(ModulePaths)`; the `anyhow::Result` return exists only to
/// mirror [`build_module_tree`] for API continuity.
///
/// [`build_module_tree`]: super::build_module_tree
pub fn build_module_paths(root: &Path, files: &[ParsedFile]) -> anyhow::Result<ModulePaths> {
    // 1. Index files by resolved path, mirroring build_module_tree.
    let by_path: AHashMap<PathBuf, &ParsedFile> =
        files.iter().map(|f| (f.path.clone(), f)).collect();
    let known_files: HashSet<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    // 2. Same Vec-based stack walk as build_module_tree. Each entry also
    //    carries its name segments and a `gated` flag: every edge so far
    //    was `#[cfg(test)]`-gated.
    //
    //    First recorded path wins on duplicate edges (contains_key guard).
    //
    //    Gating semantics ("ungated wins on revisit"): a gated-first visit
    //    still expands the file, and a later ungated edge re-expands it.
    //    Segments are unaffected; the revisit only clears test-only status.
    //
    //    The fixed point marks a file production-reachable iff any ungated
    //    chain reaches it.
    let mut paths: AHashMap<PathBuf, Vec<Box<str>>> = AHashMap::new();
    let mut production: AHashSet<PathBuf> = AHashSet::new();
    // Final warning count is unknown here; Vec::new() is fine.
    let mut warnings = Vec::new();
    // Re-expansions (ungated revisit) already collected this file's
    // warnings on first visit; sink re-run resolver output here so it
    // cannot duplicate.
    let mut discarded_warnings = Vec::new();
    let mut queue: Vec<(PathBuf, Vec<Box<str>>, bool)> =
        vec![(root.to_path_buf(), Vec::new(), false)];
    while let Some((path, segments, gated)) = queue.pop() {
        let first_visit = !paths.contains_key(&path);
        let newly_production = !gated && production.insert(path.clone());
        if !first_visit && !newly_production {
            continue;
        }
        // First recorded path wins; an ungated revisit only re-expands.
        if first_visit {
            paths.insert(path.clone(), segments.clone());
        }
        let Some(pf) = by_path.get(&path) else {
            continue;
        };
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let children_dir = mod_children_dir(&path, root);
        let warning_sink = if first_visit {
            &mut warnings
        } else {
            &mut discarded_warnings
        };
        for child in resolve_mod_children(
            pf.tree.root_node(),
            parent_dir,
            &children_dir,
            &path,
            &pf.source,
            warning_sink,
            &known_files,
        ) {
            match child {
                ModChild::Inline => {}
                ModChild::File {
                    path: cpath,
                    name,
                    vis_text: _,
                    gated: child_gated,
                } => {
                    // child segments: parent's plus the declared mod name.
                    let mut child_segs = segments.clone();
                    child_segs.push(name);
                    queue.push((cpath, child_segs, gated || child_gated));
                }
            }
        }
    }
    Ok(ModulePaths {
        paths,
        production,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::{ParsedFile, build_module_paths};
    use std::path::PathBuf;

    fn src(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    /// Parse `(path, source)` pairs into [`ParsedFile`]s for
    /// [`build_module_paths`].
    fn parse_files(sources: Vec<(PathBuf, String)>) -> Vec<ParsedFile> {
        sources
            .into_iter()
            .map(|(p, s)| ParsedFile::new(p, s).expect("test source must parse"))
            .collect()
    }

    fn segs(files: &[ParsedFile], path: &str) -> Vec<String> {
        let paths = build_module_paths(&src("src/lib.rs"), files).unwrap();
        paths
            .segments_for(&src(path))
            .unwrap_or(&[])
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// `Box<str>` segments as plain strings for assertion readability.
    fn segments_as_strs(segments: &[Box<str>]) -> Vec<&str> {
        segments.iter().map(|s| &**s).collect()
    }

    #[test]
    fn module_paths_root_maps_to_empty_segments() {
        let files = parse_files(vec![(src("src/lib.rs"), "pub fn f() {}\n".into())]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        assert_eq!(
            paths.segments_for(&src("src/lib.rs")),
            Some([].as_slice()),
            "crate root has no segments"
        );
        assert!(paths.contains(&src("src/lib.rs")));
        assert!(paths.warnings().is_empty());
    }

    #[test]
    fn module_paths_resolves_plain_foo_rs() {
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        assert_eq!(segs(&files, "src/foo.rs"), vec!["foo"]);
    }

    #[test]
    fn module_paths_falls_back_to_mod_rs_when_foo_rs_absent() {
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo/mod.rs"), "pub fn f() {}\n".into()),
        ]);
        assert_eq!(segs(&files, "src/foo/mod.rs"), vec!["foo"]);
    }

    #[test]
    fn module_paths_honors_path_attr_override() {
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[path = \"nested/real.rs\"]\nmod foo;\n".into(),
            ),
            (src("src/nested/real.rs"), "pub fn f() {}\n".into()),
        ]);
        // The declared name wins even when the file path differs.
        assert_eq!(segs(&files, "src/nested/real.rs"), vec!["foo"]);
    }

    #[test]
    fn module_paths_nests_two_levels() {
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod a;\n".into()),
            (src("src/a/mod.rs"), "mod b;\n".into()),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);
        assert_eq!(segs(&files, "src/a/mod.rs"), vec!["a"]);
        assert_eq!(segs(&files, "src/a/b.rs"), vec!["a", "b"]);
    }

    #[test]
    fn module_paths_first_recorded_path_wins_on_duplicate_edges() {
        // Two `#[path]` edges reach one file; the LIFO stack records the
        // last-declared edge first, and the contains_key guard keeps it.
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[path = \"foo.rs\"]\nmod first;\n#[path = \"foo.rs\"]\nmod second;\n".into(),
            ),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        assert_eq!(segs(&files, "src/foo.rs"), vec!["second"]);
    }

    #[test]
    fn module_paths_resolves_child_of_non_mod_rs_parent_in_stem_subdir() {
        // `dir/foo.rs` resolves `mod bar;` in `dir/foo/bar.rs`.
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod builtins;\n".into()),
            (src("src/builtins.rs"), "mod capacity_tests;\n".into()),
            (
                src("src/builtins/capacity_tests.rs"),
                "pub fn f() {}\n".into(),
            ),
        ]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        assert_eq!(
            segs(&files, "src/builtins/capacity_tests.rs"),
            vec!["builtins", "capacity_tests"],
        );
        assert!(
            paths.warnings().is_empty(),
            "stem-subdir child must not warn: {:?}",
            paths.warnings()
        );
    }

    #[test]
    fn module_paths_crate_root_children_resolve_beside_root() {
        let files = parse_files(vec![
            (src("src/main.rs"), "mod util;\n".into()),
            (src("src/util.rs"), "pub fn f() {}\n".into()),
        ]);
        let paths = build_module_paths(&src("src/main.rs"), &files).unwrap();
        let segs: Vec<&str> = paths
            .segments_for(&src("src/util.rs"))
            .expect("util.rs in tree")
            .iter()
            .map(|s| &**s)
            .collect();
        assert_eq!(segs, ["util"]);
    }

    #[test]
    fn module_paths_marks_gated_chain_test_only() {
        // A gated `mod` declaration makes the child (and its transitive
        // children) test-only.
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[cfg(test)]\nmod ctx;\nmod prod;\n".into(),
            ),
            (src("src/ctx.rs"), "mod helper;\n".into()),
            (src("src/ctx/helper.rs"), "pub fn h() {}\n".into()),
            (src("src/prod.rs"), "pub fn p() {}\n".into()),
        ]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        assert!(paths.is_test_only(&src("src/ctx.rs")), "gated child");
        assert!(
            paths.is_test_only(&src("src/ctx/helper.rs")),
            "gating is transitive"
        );
        assert!(!paths.is_test_only(&src("src/prod.rs")), "ungated child");
        assert!(!paths.is_test_only(&src("src/lib.rs")), "crate root");
    }

    #[test]
    fn module_paths_ungated_edge_wins_over_gated_revisit() {
        // A file reached through both a gated and an ungated edge stays
        // production-reachable ("ungated wins on revisit").
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[cfg(test)]\n#[path = \"shared.rs\"]\nmod t;\n\
                 #[path = \"shared.rs\"]\nmod p;\n"
                    .into(),
            ),
            (src("src/shared.rs"), "pub fn f() {}\n".into()),
        ]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        assert!(
            !paths.is_test_only(&src("src/shared.rs")),
            "one ungated edge is enough"
        );
    }

    #[test]
    fn module_paths_gated_first_revisit_keeps_segments_and_unique_warnings() {
        // LIFO pops the gated edge (declared last) first; the ungated
        // revisit re-expands the file without duplicating warnings.
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[path = \"shared.rs\"]\nmod p;\n\
                 #[cfg(test)]\n#[path = \"shared.rs\"]\nmod t;\n"
                    .into(),
            ),
            (src("src/shared.rs"), "mod inner;\nmod missing;\n".into()),
            (src("src/shared/inner.rs"), "pub fn f() {}\n".into()),
        ]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();

        assert!(!paths.is_test_only(&src("src/shared.rs")));
        assert!(!paths.is_test_only(&src("src/shared/inner.rs")));

        // First recorded path wins: the LIFO stack expanded the
        // last-declared gated edge first, naming the module `t`.
        assert_eq!(
            paths
                .segments_for(&src("src/shared.rs"))
                .map(segments_as_strs),
            Some(vec!["t"])
        );
        assert_eq!(
            paths
                .segments_for(&src("src/shared/inner.rs"))
                .map(segments_as_strs),
            Some(vec!["t", "inner"])
        );

        // `shared.rs` expands twice, yet the walk collects its
        // unresolved `missing` warning exactly once.
        assert_eq!(paths.warnings().len(), 1, "{:?}", paths.warnings());
        assert!(
            paths.warnings()[0].contains("missing"),
            "the warning names the unresolved mod: {:?}",
            paths.warnings()
        );
    }

    #[test]
    fn module_paths_path_attr_from_non_root_file_joins_its_own_directory() {
        // `#[path]` in a mid-tree non-`mod.rs` file resolves beside that
        // file, not inside its `foo/` children directory.
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo.rs"), "#[path = \"bar.rs\"]\nmod b;\n".into()),
            (src("src/bar.rs"), "pub fn f() {}\n".into()),
        ]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        assert_eq!(
            paths.segments_for(&src("src/bar.rs")).map(segments_as_strs),
            Some(vec!["foo", "b"])
        );
        assert!(paths.warnings().is_empty(), "{:?}", paths.warnings());
    }

    #[test]
    fn module_paths_unresolved_mod_records_warning() {
        let files = parse_files(vec![(
            src("src/lib.rs"),
            "mod missing;\n#[path = \"nope.rs\"]\nmod gone;\n".into(),
        )]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        assert!(
            paths
                .warnings()
                .iter()
                .any(|s| s.contains("mod missing") && s.contains("resolves to no")),
            "unresolved mod warning: {:?}",
            paths.warnings()
        );
    }

    #[test]
    fn module_paths_iter_yields_every_resolved_file() {
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod a;\n".into()),
            (src("src/a/mod.rs"), "mod b;\n".into()),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);
        let paths = build_module_paths(&src("src/lib.rs"), &files).unwrap();
        // Compare paths, not strings: on Windows the resolver joins children
        // with `\` separators, and `Path` equality ignores that difference.
        let mut got: Vec<(PathBuf, usize)> = paths
            .iter()
            .map(|(p, s)| (p.to_path_buf(), s.len()))
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                (src("src/a/b.rs"), 2),
                (src("src/a/mod.rs"), 1),
                (src("src/lib.rs"), 0),
            ],
        );
    }
}
