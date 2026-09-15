//! Module-path segments per file, for crate-aware visibility narrowing.
//!
//! Maps each reachable source file to the declared `mod` name segments from
//! the crate root, reusing the exact resolution rules of `modules.rs`
//! (`build_module_tree`).
//!
//! Edition `foo.rs` preference, `foo/mod.rs` fallback, and `#[path]`
//! overrides all apply.

use super::ParsedFile;
use super::modules::{ModChild, resolve_mod_children};
use ahash::AHashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

    // 2. Same Vec-based stack walk as build_module_tree, but each entry also
    //    carries its accumulated name segments. First recorded path wins on
    //    duplicate edges (contains_key guard).
    let mut paths: AHashMap<PathBuf, Vec<Box<str>>> = AHashMap::new();
    // Final warning count is unknown here; Vec::new() is fine.
    let mut warnings = Vec::new();
    let mut queue: Vec<(PathBuf, Vec<Box<str>>)> = vec![(root.to_path_buf(), Vec::new())];
    while let Some((path, segments)) = queue.pop() {
        if paths.contains_key(&path) {
            continue;
        }
        paths.insert(path.clone(), segments.clone());
        let Some(pf) = by_path.get(&path) else {
            continue;
        };
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
        for child in resolve_mod_children(
            pf.tree.root_node(),
            parent_dir,
            &path,
            &pf.source,
            &mut warnings,
            &known_files,
        ) {
            match child {
                ModChild::Inline => {}
                ModChild::File {
                    path: cpath,
                    name,
                    vis_text: _,
                } => {
                    // child segments: parent's plus the declared mod name.
                    let mut child_segs = segments.clone();
                    child_segs.push(name);
                    queue.push((cpath, child_segs));
                }
            }
        }
    }
    Ok(ModulePaths { paths, warnings })
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
