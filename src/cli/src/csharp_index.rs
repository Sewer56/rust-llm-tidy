//! C# project-reference scope and source-versioned parse cache for indexed linting.

use crate::paths;
use rayon::prelude::*;
use rust_llm_tidy_lang::backends::CanThrowIndex;
use rust_llm_tidy_model::parse::ParseResult;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Run-owned C# parses and shared throw answers.
#[derive(Default)]
pub(crate) struct CSharpIndex {
    parses: HashMap<PathBuf, ParseResult>,
    /// Qualified throw answers for the current cached source versions.
    pub(crate) index: CanThrowIndex,
}

impl CSharpIndex {
    /// Parse the project-reference scope of `inputs`, retaining successful parses.
    ///
    /// # Errors
    /// Returns an error when collecting project source entries fails.
    pub(crate) fn build(inputs: &[PathBuf]) -> anyhow::Result<Self> {
        let files = project_scope(inputs)?;

        let parse = |path: &PathBuf| {
            let source = std::fs::read_to_string(path).ok()?;
            let parsed = rust_llm_tidy_lang::backend_for("cs")?.parse(&source).ok()?;
            Some((path.clone(), parsed))
        };
        let parses = if crate::pipeline::should_parallelize(&files) {
            files.par_iter().filter_map(parse).collect()
        } else {
            files.iter().filter_map(parse).collect()
        };

        let mut cache = Self {
            parses,
            index: CanThrowIndex::default(),
        };
        cache.index = CanThrowIndex::from_parses(cache.parses.values());
        Ok(cache)
    }

    /// Refresh changed `inputs` after mutations, then rebuild affected throw answers.
    /// Unreadable sources lose their cached facts and fail in the per-file lint pass.
    pub(crate) fn refresh(&mut self, inputs: &[PathBuf]) {
        let mut changed = false;
        for path in inputs {
            if !paths::ext_in(path.extension().and_then(|e| e.to_str()), &["cs"]) {
                continue;
            }
            let key = path.canonicalize().unwrap_or_else(|_| path.clone());
            let source = std::fs::read_to_string(path).ok();
            if source.as_deref() == self.parses.get(&key).map(|p| p.source.as_str()) {
                continue;
            }

            changed = true;
            self.parses.remove(&key);
            if let Some(source) = source
                && let Some(backend) = rust_llm_tidy_lang::backend_for("cs")
                && let Ok(parsed) = backend.parse(&source)
            {
                self.parses.insert(key, parsed);
            }
        }
        if changed {
            self.index = CanThrowIndex::from_parses(self.parses.values());
        }
    }

    /// Return the cached parse for the canonical identity of `path`.
    pub(crate) fn parsed(&self, path: &Path) -> Option<&ParseResult> {
        self.parses.get(&path.canonicalize().ok()?)
    }
}

/// Resolve nearest projects and their literal reference closure for C# `inputs`.
///
/// # Errors
/// Returns an error when a project's source directory entries cannot be read.
fn project_scope(inputs: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = HashSet::new();
    let mut pending = Vec::new();
    let mut nearest: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in inputs {
        if !paths::ext_in(path.extension().and_then(|e| e.to_str()), &["cs"]) {
            continue;
        }
        let path = path.canonicalize().unwrap_or_else(|_| path.clone());
        files.insert(path.clone());
        if let Some(parent) = path.parent() {
            if nearest.contains_key(parent) {
                continue;
            }
            let mut searched = Vec::new();
            let mut found = Vec::new();
            for dir in parent.ancestors() {
                if let Some(projects) = nearest.get(dir) {
                    found = projects.clone();
                    break;
                }
                searched.push(dir.to_path_buf());
                let mut projects: Vec<_> = std::fs::read_dir(dir)
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "csproj") && p.is_file())
                    .collect();
                if !projects.is_empty() {
                    projects.sort();
                    found = projects;
                    break;
                }
                if dir.join(".git").exists() {
                    break;
                }
            }
            for dir in searched {
                nearest.insert(dir, found.clone());
            }
            pending.extend(found);
        }
    }

    let mut visited = HashSet::new();
    while let Some(project) = pending.pop() {
        let Ok(project) = project.canonicalize() else {
            continue;
        };
        if !visited.insert(project.clone()) {
            continue;
        }
        let Some(dir) = project.parent() else {
            continue;
        };
        let Ok(source) = std::fs::read_to_string(&project) else {
            continue;
        };
        let mut sources = Vec::new();
        paths::collect_project_files(dir, &["cs"], &mut sources, true)?;
        files.extend(sources.into_iter().filter_map(|p| p.canonicalize().ok()));

        for tag in source.split("<ProjectReference").skip(1) {
            if !tag.starts_with(char::is_whitespace) {
                continue;
            }
            let Some((tag, _)) = tag.split_once('>') else {
                continue;
            };
            let Some((_, include)) = tag.split_once("Include=\"") else {
                continue;
            };
            let Some((include, _)) = include.split_once('"') else {
                continue;
            };
            if include.contains(['$', '*', '?']) {
                continue;
            }
            pending.push(dir.join(include.replace('\\', "/")));
        }
    }

    let mut files: Vec<_> = files.into_iter().collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nearest projects close over literal references without crossing ignored or nested repository boundaries.
    #[test]
    fn scope_should_follow_reference_cycles_and_skip_unresolvable_targets() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-scope-{}", std::process::id()));
        let _guard = Directory(root.clone());
        for dir in [
            ".git",
            "a/src",
            "a/nested/.git",
            "a/ignored",
            "b",
            "c",
            "loose",
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        // project_scope canonicalizes its results; align expectations when the
        // platform temp dir is a symlink (macOS /var -> /private/var).
        let root = root.canonicalize().unwrap();
        for path in [
            "a/src/A.cs",
            "a/nested/Hidden.cs",
            "a/ignored/Hidden.cs",
            "b/B.cs",
            "c/C.cs",
            "loose/L.cs",
        ] {
            std::fs::write(root.join(path), "class C {}").unwrap();
        }
        std::fs::write(root.join(".gitignore"), "a/ignored/\n").unwrap();
        std::fs::write(root.join("a/a.csproj"), "<ProjectReference Include=\"../b/b.csproj\"/><ProjectReference Include=\"missing.csproj\"/>").unwrap();
        std::fs::write(
            root.join("b/b.csproj"),
            "<ProjectReference Include=\"../c/c.csproj\"/>",
        )
        .unwrap();
        std::fs::write(
            root.join("c/c.csproj"),
            "<ProjectReference Include=\"../a/a.csproj\"/>",
        )
        .unwrap();

        let loose = project_scope(&[root.join("loose/L.cs")]).unwrap();
        std::fs::write(root.join("outer.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("Outer.cs"), "class Outer {}").unwrap();
        let scope = project_scope(&[root.join("a/src/A.cs")]).unwrap();

        assert_eq!(
            scope,
            ["a/src/A.cs", "b/B.cs", "c/C.cs"].map(|p| root.join(p))
        );
        assert_eq!(loose, [root.join("loose/L.cs")]);
    }

    /// Changed sources replace throw evidence while unchanged sources retain their cached syntax tree.
    #[test]
    fn refresh_should_replace_changed_facts_and_reuse_unchanged_parses() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-refresh-{}", std::process::id()));
        let _guard = Directory(root.clone());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".git"), "").unwrap();
        let caller = root.join("A.cs");
        let helper = root.join("T.cs");
        std::fs::write(&caller, "class A { public void Caller() { T.Helper(); } }").unwrap();
        std::fs::write(&helper, "class T { void Helper() { throw new E(); } }").unwrap();
        let paths = [caller.clone(), helper.clone()];
        let mut cache = CSharpIndex::build(&paths).unwrap();
        let backend = rust_llm_tidy_lang::backend_for("cs").unwrap();
        let initial = backend.lint_indexed(cache.parsed(&caller).unwrap(), &cache.index);
        let tree = cache
            .parsed(&caller)
            .unwrap()
            .syntax_tree()
            .root_node()
            .id();
        std::fs::write(&helper, "class T { void Helper() {} }").unwrap();

        cache.refresh(&paths);
        let refreshed = backend.lint_indexed(cache.parsed(&caller).unwrap(), &cache.index);

        assert!(initial.iter().any(|d| d.code == "DOC002"));
        assert!(!refreshed.iter().any(|d| d.code == "DOC002"));
        assert_eq!(
            cache
                .parsed(&caller)
                .unwrap()
                .syntax_tree()
                .root_node()
                .id(),
            tree
        );
    }

    struct Directory(PathBuf);

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
