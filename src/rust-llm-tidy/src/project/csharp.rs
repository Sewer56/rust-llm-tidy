//! C# project-reference scope and source-versioned parse cache for indexed linting.

use crate::input as paths;
use crate::languages::{CanThrowIndex, backend_for};
use crate::pipeline;
use crate::source::ParseResult;
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use rayon::prelude::*;
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
            let parsed = backend_for("cs")?.parse(&source).ok()?;
            Some((path.clone(), parsed))
        };
        let parses = if pipeline::should_parallelize(&files) {
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
            // Deleted sources keep their resolved cache key so refresh drops the stale parse.
            let key = path
                .canonicalize()
                .unwrap_or_else(|_| missing_source_key(path));
            let source = std::fs::read_to_string(path).ok();
            if source.as_deref() == self.parses.get(&key).map(|p| p.source.as_str()) {
                continue;
            }

            changed = true;
            self.parses.remove(&key);
            if let Some(source) = source
                && let Some(backend) = backend_for("cs")
                && let Ok(parsed) = backend.parse(&source)
            {
                self.parses.insert(key, parsed);
            }
        }
        if changed {
            self.index = CanThrowIndex::from_parses(self.parses.values());
        }
    }

    /// Return the cached parse for the resolved absolute identity of `path`.
    pub(crate) fn parsed(&self, path: &Path) -> Option<&ParseResult> {
        self.parses.get(&path.canonicalize().ok()?)
    }
}

/// Reproduce the resolved cache key a since-deleted `path` received from
/// [`project_scope`], so refresh still invalidates its cached parse.
///
/// Resolves the surviving parent directory and rejoins the file name;
/// bare file names anchor at the current directory instead.
fn missing_source_key(path: &Path) -> PathBuf {
    let Some(name) = path.file_name() else {
        return path.to_path_buf();
    };
    let parent = path.parent().unwrap_or(Path::new(""));
    let anchor = if parent.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_default()
    } else {
        parent.to_path_buf()
    };
    anchor.canonicalize().unwrap_or(anchor).join(name)
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
        paths::collect_project_files(dir, &["cs"], &mut sources, true, true)?;
        files.extend(sources.into_iter().filter_map(|p| p.canonicalize().ok()));

        for include in literal_project_includes(&source) {
            pending.push(dir.join(include.replace('\\', "/")));
        }
    }

    let mut files: Vec<_> = files.into_iter().collect();
    files.sort();
    Ok(files)
}

/// Extract the literal `Include` targets of `<ProjectReference>` elements
/// from project XML.
///
/// Element and attribute local names are matched so namespaces never hide a
/// reference.
///
/// Commented-out references contribute nothing, and targets containing
/// MSBuild properties or wildcards (`$`, `*`, `?`) are skipped as
/// non-literal. XML parsing stops at malformed content, keeping the targets
/// read before it.
fn literal_project_includes(source: &str) -> Vec<String> {
    let mut reader = Reader::from_str(source);

    // Project fragments may nest or close loosely; only element shape matters here.
    reader.config_mut().check_end_names = false;
    let mut includes = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if e.name().local_name().as_ref() != "ProjectReference" {
                    continue;
                }
                for attr in e.attributes().flatten() {
                    if attr.key.local_name().as_ref() == "Include"
                        && let Ok(value) = attr.normalized_value(XmlVersion::Implicit1_0)
                        && !value.contains(['$', '*', '?'])
                    {
                        includes.push(value.into_owned());
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    includes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nearest projects close over literal references without crossing ignored
    /// or nested repository boundaries.
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

    /// Include targets survive quote style, spaced equals signs, paired
    /// elements, prefixed names, backslashes, and malformed tails.
    ///
    /// Commented-out references and MSBuild-variable includes stay out of scope.
    #[test]
    fn scope_should_read_xml_includes_and_skip_commented_or_variable_references() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-xml-{}", std::process::id()));
        let _guard = Directory(root.clone());
        for dir in ["a", "b", "c", "commented", "vars"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        // project_scope canonicalizes its results; align expectations when the
        // platform temp dir is a symlink (macOS /var -> /private/var).
        let root = root.canonicalize().unwrap();
        for path in [
            "a/A.cs",
            "b/B.cs",
            "c/C.cs",
            "commented/Commented.cs",
            "vars/V.cs",
        ] {
            std::fs::write(root.join(path), "class C {}").unwrap();
        }
        // The commented and $(V) projects exist on disk so skipping their
        // references is observable in the asserted scope.
        std::fs::write(root.join("b/b.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("c/c.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("commented/commented.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("vars/$(V).csproj"), "<Project />").unwrap();
        std::fs::write(
            root.join("a/a.csproj"),
            r#"<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003"
              xmlns:msb="http://schemas.microsoft.com/developer/msbuild/2003">
  <!-- <ProjectReference Include="../commented/commented.csproj" /> -->
  <ItemGroup>
    <ProjectReference Include = "..\b\b.csproj"><Project>../b/b.csproj</Project></ProjectReference>
    <msb:ProjectReference Include='../c/c.csproj' />
    <ProjectReference Include="../vars/$(V).csproj" />
  </ItemGroup>
</Project>
<ProjectReference Include="../trunc"#,
        )
        .unwrap();

        let scope = project_scope(&[root.join("a/A.cs")]).unwrap();

        assert_eq!(scope, ["a/A.cs", "b/B.cs", "c/C.cs"].map(|p| root.join(p)));
    }

    /// Changed sources replace throw evidence while unchanged sources retain
    /// their cached syntax tree.
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
        let backend = crate::languages::backend_for("cs").unwrap();
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

    /// Deleted helper sources drop their throw facts from the shared index,
    /// even when the supplied inputs are relative paths.
    #[test]
    fn refresh_should_drop_deleted_source_facts_for_relative_inputs() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-relative-{}", std::process::id()));
        let _guard = Directory(root.clone());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".git"), "").unwrap();
        let caller = root.join("A.cs");
        let helper = root.join("T.cs");
        std::fs::write(&caller, "class A { public void Caller() { T.Helper(); } }").unwrap();
        std::fs::write(&helper, "class T { void Helper() { throw new E(); } }").unwrap();

        // Explicit CLI inputs arrive as given: relative to the cwd.
        let inputs = [path_from_cwd(&caller), path_from_cwd(&helper)];
        let mut cache = CSharpIndex::build(&inputs).unwrap();
        let backend = crate::languages::backend_for("cs").unwrap();
        let initial = backend.lint_indexed(cache.parsed(&caller).unwrap(), &cache.index);
        std::fs::remove_file(&helper).unwrap();

        cache.refresh(&inputs);
        let refreshed = backend.lint_indexed(cache.parsed(&caller).unwrap(), &cache.index);

        assert!(initial.iter().any(|d| d.code == "DOC002"));
        assert!(!refreshed.iter().any(|d| d.code == "DOC002"));
    }

    /// Express `target` relative to the current directory without mutating it,
    /// mirroring how explicit CLI inputs reach the pipeline.
    fn path_from_cwd(target: &Path) -> PathBuf {
        let cwd = std::env::current_dir().unwrap();
        let cwd: Vec<_> = cwd.components().collect();
        let target_parts: Vec<_> = target.components().collect();
        let common = cwd
            .iter()
            .zip(&target_parts)
            .take_while(|(a, b)| **a == **b)
            .count();

        let mut rel = PathBuf::new();
        for _ in common..cwd.len() {
            rel.push("..");
        }
        for part in &target_parts[common..] {
            rel.push(part.as_os_str());
        }
        rel
    }

    struct Directory(PathBuf);

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
