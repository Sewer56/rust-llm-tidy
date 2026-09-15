//! C# project-reference scope and source-versioned parse cache for indexed linting.

use crate::input as paths;
use crate::languages::{CanThrowIndex, backend_for};
use crate::pipeline;
use crate::source::ParseResult;
use ahash::{AHashMap, AHashSet};
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Run-owned C# parses, shared throw answers, and analysis scopes.
#[derive(Default)]
pub(crate) struct CSharpIndex {
    parses: HashMap<PathBuf, ParseResult>,
    /// Qualified throw answers for the current cached source versions.
    pub(crate) index: CanThrowIndex,
    /// Per-project scopes and file ownership for indexed linting.
    scopes: CSharpScopes,
}

/// Files collected under project directories, keyed by project path.
type ProjectFiles = HashMap<PathBuf, Vec<PathBuf>>;

/// Project-backed inputs paired with their nearest projects.
type RoutedInputs = Vec<(PathBuf, Vec<PathBuf>)>;

/// Per-project analysis scopes plus file ownership for indexed linting.
///
/// One scope covers each nearest project's reference closure;
/// project-less inputs share one loose scope.
///
/// Closures overlap, so the owner map resolves each file to one
/// scope: the deepest project that scanned it, or the loose scope.
#[derive(Default)]
pub(crate) struct CSharpScopes {
    /// Project scopes sorted by project path, loose scope last.
    scopes: Vec<CSharpScope>,
    /// Scoped file -> owning project; loose files never appear here.
    owners: AHashMap<PathBuf, PathBuf>,
    /// Project-less input files (the loose scope's members).
    loose: AHashSet<PathBuf>,
}

/// One project's analysis scope.
pub(crate) struct CSharpScope {
    /// The anchoring project; `None` for the loose scope.
    pub(crate) project: Option<PathBuf>,
    /// The scope's files, sorted.
    pub(crate) files: Vec<PathBuf>,
}

impl CSharpIndex {
    /// Parse the project-reference scope of `inputs`, retaining successful parses.
    ///
    /// # Errors
    /// Returns an error when collecting project source entries fails.
    pub(crate) fn build(inputs: &[PathBuf]) -> anyhow::Result<Self> {
        let scopes = project_scope(inputs)?;
        let files = scopes.flat_files();

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
            scopes,
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
    ///
    /// Resolved spellings (scope files) hit the exact key; only other
    /// spellings pay for the `canonicalize` fallback.
    pub(crate) fn parsed(&self, path: &Path) -> Option<&ParseResult> {
        if let Some(parsed) = self.parses.get(path) {
            return Some(parsed);
        }
        self.parses.get(&path.canonicalize().ok()?)
    }

    /// The per-project analysis scopes discovered for this run.
    ///
    /// Path-level facts: unlike the parses, they stay valid through
    /// [`CSharpIndex::refresh`], which only swaps source versions.
    pub(crate) fn scopes(&self) -> &CSharpScopes {
        &self.scopes
    }
}

impl CSharpScopes {
    /// The scopes: project scopes in sorted project order, loose last.
    pub(crate) fn list(&self) -> &[CSharpScope] {
        &self.scopes
    }

    /// Whether `project`'s scope owns `file` (`None` = loose).
    pub(crate) fn owned_by(&self, file: &Path, project: Option<&Path>) -> bool {
        match project {
            Some(project) => self.owners.get(file).is_some_and(|owner| owner == project),
            None => self.loose.contains(file),
        }
    }

    /// The sorted union of every scope's files; the run's parse set.
    fn flat_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .scopes
            .iter()
            .flat_map(|scope| scope.files.iter().cloned())
            .collect();
        files.sort();
        files.dedup();
        files
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
/// Discovery walks in three phases:
///
/// - each input's nearest projects (memoized walk-up that stops at
///   repository boundaries),
/// - one scan per reachable project (own files plus literal
///   references),
/// - one scope per root project (its reference closure) plus a loose
///   scope for project-less inputs.
///
/// Explicit inputs join a scanned project's own files even when a
/// `.gitignore` rule hides them from the project scan, so explicitly
/// named files always parse.
///
/// # Errors
/// Returns an error when a project's source directory entries cannot be read.
fn project_scope(inputs: &[PathBuf]) -> anyhow::Result<CSharpScopes> {
    let (roots, routed, mut loose) = nearest_roots(inputs);
    let (mut own_files, references) = scan_projects(&roots)?;
    // Explicit inputs survive ignore rules: each joins its first
    // scanned project's files, or goes loose when no scan collected it.
    for (input, projects) in routed {
        if let Some(project) = projects
            .iter()
            .find(|project| own_files.contains_key(*project))
            && let Some(files) = own_files.get_mut(project)
        {
            files.push(input);
        } else if !own_files.values().any(|files| files.contains(&input)) {
            loose.push(input);
        }
    }
    Ok(build_scopes(roots, own_files, references, loose))
}

/// Phase 3: one scope per root (sorted), plus the loose scope.
fn build_scopes(
    roots: Vec<PathBuf>,
    own_files: ProjectFiles,
    references: ProjectFiles,
    mut loose: Vec<PathBuf>,
) -> CSharpScopes {
    // Owners: the deepest scanning project wins; path order breaks
    // ties, so unordered iteration stays deterministic.
    let mut owners: AHashMap<PathBuf, PathBuf> = AHashMap::new();
    for (project, files) in &own_files {
        let depth = dir_depth(project);
        for file in files {
            let replace = match owners.get(file) {
                None => true,
                Some(previous) => {
                    let previous_depth = dir_depth(previous);
                    depth > previous_depth || (depth == previous_depth && project < previous)
                }
            };
            if replace {
                owners.insert(file.clone(), project.clone());
            }
        }
    }

    let mut sorted_roots = roots;
    sorted_roots.sort();
    let mut scopes: Vec<CSharpScope> =
        Vec::with_capacity(sorted_roots.len() + usize::from(!loose.is_empty()));
    for root in &sorted_roots {
        // Closure membership: every project reachable from this root.
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut stack = vec![root.clone()];
        let mut files: Vec<PathBuf> = Vec::new();
        while let Some(project) = stack.pop() {
            if !seen.insert(project.clone()) {
                continue;
            }
            if let Some(own) = own_files.get(&project) {
                files.extend(own.iter().cloned());
            }
            if let Some(found) = references.get(&project) {
                stack.extend(found.iter().cloned());
            }
        }
        files.sort();
        files.dedup();
        scopes.push(CSharpScope {
            project: Some(root.clone()),
            files,
        });
    }
    let mut loose_set: AHashSet<PathBuf> = AHashSet::new();
    if !loose.is_empty() {
        loose_set.extend(loose.iter().cloned());
        loose.sort();
        loose.dedup();
        scopes.push(CSharpScope {
            project: None,
            files: loose,
        });
    }

    CSharpScopes {
        scopes,
        owners,
        loose: loose_set,
    }
}

/// Phase 1: each input's nearest projects, or the loose set.
///
/// Returns the distinct canonical root projects in first-seen order,
/// one `(input, projects)` pair per project-backed input, and the
/// project-less inputs.
fn nearest_roots(inputs: &[PathBuf]) -> (Vec<PathBuf>, RoutedInputs, Vec<PathBuf>) {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut routed: RoutedInputs = Vec::new();
    let mut loose: Vec<PathBuf> = Vec::new();
    let mut root_seen: HashSet<PathBuf> = HashSet::new();
    // Memoized per-directory nearest projects, in raw read_dir spelling.
    let mut nearest: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in inputs {
        if !paths::ext_in(path.extension().and_then(|e| e.to_str()), &["cs"]) {
            continue;
        }
        let path = path.canonicalize().unwrap_or_else(|_| path.clone());
        let mut found: Vec<PathBuf> = Vec::new();
        if let Some(parent) = path.parent() {
            if let Some(projects) = nearest.get(parent) {
                found = projects.clone();
            } else {
                let mut searched = Vec::new();
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
            }
        }
        // Unresolvable projects drop out here, as they would in the scan.
        let found: Vec<PathBuf> = found
            .iter()
            .filter_map(|project| project.canonicalize().ok())
            .collect();
        if found.is_empty() {
            loose.push(path);
        } else {
            for project in &found {
                if root_seen.insert(project.clone()) {
                    roots.push(project.clone());
                }
            }
            routed.push((path, found));
        }
    }
    (roots, routed, loose)
}

/// Phase 2: scan each reachable project once.
///
/// Returns each scanned project's own files and its canonical literal
/// references.
///
/// # Errors
/// Returns an error when a project's source directory entries cannot be read.
fn scan_projects(roots: &[PathBuf]) -> anyhow::Result<(ProjectFiles, ProjectFiles)> {
    let mut own_files: ProjectFiles = HashMap::new();
    let mut references: ProjectFiles = HashMap::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = roots.iter().cloned().collect();
    while let Some(project) = queue.pop_front() {
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
        own_files.insert(
            project.clone(),
            sources
                .into_iter()
                .filter_map(|p| p.canonicalize().ok())
                .collect(),
        );

        let mut found: Vec<PathBuf> = literal_project_includes(&source)
            .into_iter()
            .filter_map(|include| dir.join(include.replace('\\', "/")).canonicalize().ok())
            .collect();
        found.sort();
        queue.extend(found.iter().cloned());
        references.insert(project, found);
    }
    Ok((own_files, references))
}

/// The component depth of `project`'s directory, for owner conflicts.
fn dir_depth(project: &Path) -> usize {
    project
        .parent()
        .map(|dir| dir.components().count())
        .unwrap_or(0)
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

    /// The sorted files of one project's scope (`None` = loose).
    fn scope_files<'a>(scopes: &'a CSharpScopes, project: Option<&Path>) -> &'a [PathBuf] {
        scopes
            .list()
            .iter()
            .find(|scope| scope.project.as_deref() == project)
            .map(|scope| scope.files.as_slice())
            .unwrap_or(&[])
    }

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
        let scopes = project_scope(&[root.join("a/src/A.cs")]).unwrap();

        // a's closure follows the reference cycle a -> b -> c -> a once.
        assert_eq!(
            scope_files(&scopes, Some(&root.join("a/a.csproj"))),
            ["a/src/A.cs", "b/B.cs", "c/C.cs"].map(|p| root.join(p))
        );
        assert_eq!(scope_files(&loose, None), [root.join("loose/L.cs")]);
        // The project that scans each closure file owns it.
        for (file, project) in [
            ("a/src/A.cs", "a/a.csproj"),
            ("b/B.cs", "b/b.csproj"),
            ("c/C.cs", "c/c.csproj"),
        ] {
            assert!(
                scopes.owned_by(&root.join(file), Some(&root.join(project))),
                "{file} owned by {project}"
            );
        }
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

        let scopes = project_scope(&[root.join("a/A.cs")]).unwrap();

        assert_eq!(
            scope_files(&scopes, Some(&root.join("a/a.csproj"))),
            ["a/A.cs", "b/B.cs", "c/C.cs"].map(|p| root.join(p))
        );
    }

    /// Two input projects get one scope each; the file's own project
    /// owns a shared referenced file, so overlaps never
    /// double-report.
    #[test]
    fn scope_should_emit_one_scope_per_input_project_and_route_shared_files_to_their_owner() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-two-{}", std::process::id()));
        let _guard = Directory(root.clone());
        for dir in ["x", "y"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        // project_scope canonicalizes its results; align expectations when the
        // platform temp dir is a symlink (macOS /var -> /private/var).
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("x/X.cs"), "class X { }").unwrap();
        std::fs::write(root.join("y/Y.cs"), "class Y { }").unwrap();
        std::fs::write(root.join("x/x.csproj"), "<Project />").unwrap();
        std::fs::write(
            root.join("y/y.csproj"),
            "<ProjectReference Include=\"../x/x.csproj\"/>",
        )
        .unwrap();

        // Input order must not matter: scopes sort by project path.
        let scopes = project_scope(&[root.join("y/Y.cs"), root.join("x/X.cs")]).unwrap();

        assert_eq!(scopes.list().len(), 2, "one scope per input project");
        assert_eq!(
            scopes.list()[0].project.as_deref(),
            Some(root.join("x/x.csproj").as_path())
        );
        assert_eq!(
            scope_files(&scopes, Some(&root.join("x/x.csproj"))),
            [root.join("x/X.cs")]
        );
        assert_eq!(
            scope_files(&scopes, Some(&root.join("y/y.csproj"))),
            [root.join("x/X.cs"), root.join("y/Y.cs")]
        );
        // The shared file belongs to its own project's scope only.
        assert!(scopes.owned_by(&root.join("x/X.cs"), Some(&root.join("x/x.csproj"))));
        assert!(!scopes.owned_by(&root.join("x/X.cs"), Some(&root.join("y/y.csproj"))));
    }

    /// An explicitly-named input survives ignore rules: it joins its
    /// project's scope and the parse set even when `.gitignore` hides
    /// it from the scan.
    #[test]
    fn scope_should_keep_explicit_input_when_gitignore_hides_it_from_the_scan() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-ignored-{}", std::process::id()));
        let _guard = Directory(root.clone());
        for dir in [".git", "p/hidden"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join(".gitignore"), "p/hidden/\n").unwrap();
        std::fs::write(root.join("p/p.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("p/V.cs"), "class V { }").unwrap();
        std::fs::write(root.join("p/hidden/H.cs"), "class H { }").unwrap();

        let scopes = project_scope(&[root.join("p/hidden/H.cs"), root.join("p/V.cs")]).unwrap();

        // The explicit hidden input stays in its project's scope,
        // anchored to the owning project.
        assert_eq!(
            scope_files(&scopes, Some(&root.join("p/p.csproj"))),
            [root.join("p/V.cs"), root.join("p/hidden/H.cs")]
        );
        assert!(scopes.owned_by(&root.join("p/hidden/H.cs"), Some(&root.join("p/p.csproj"))));
        assert!(scopes.flat_files().contains(&root.join("p/hidden/H.cs")));
    }

    /// The deepest scanning project owns a file shared with an outer
    /// project's recursive scan.
    #[test]
    fn scope_should_pick_deepest_project_when_nested_project_shares_a_directory() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-nested-{}", std::process::id()));
        let _guard = Directory(root.clone());
        for dir in ["outer", "outer/inner"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("outer/outer.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("outer/inner/inner.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("outer/O.cs"), "class O { }").unwrap();
        std::fs::write(root.join("outer/inner/I.cs"), "class I { }").unwrap();

        let scopes =
            project_scope(&[root.join("outer/O.cs"), root.join("outer/inner/I.cs")]).unwrap();

        // Both scans see the nested file; the nested project owns it.
        assert!(scopes.owned_by(
            &root.join("outer/inner/I.cs"),
            Some(&root.join("outer/inner/inner.csproj"))
        ));
        assert!(!scopes.owned_by(
            &root.join("outer/inner/I.cs"),
            Some(&root.join("outer/outer.csproj"))
        ));
        // The outer project still owns its own file.
        assert!(scopes.owned_by(
            &root.join("outer/O.cs"),
            Some(&root.join("outer/outer.csproj"))
        ));
    }

    /// Two projects sharing one directory tie on depth; the
    /// lexicographically smaller project path owns the shared files.
    #[test]
    fn scope_should_pick_smaller_project_when_two_projects_share_a_directory() {
        let root = std::env::temp_dir().join(format!("rlt-csharp-tie-{}", std::process::id()));
        let _guard = Directory(root.clone());
        std::fs::create_dir_all(root.join("p")).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("p/a.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("p/b.csproj"), "<Project />").unwrap();
        std::fs::write(root.join("p/F.cs"), "class F { }").unwrap();

        let scopes = project_scope(&[root.join("p/F.cs")]).unwrap();

        // Both projects scan the shared directory; `a` wins the tie.
        assert!(scopes.owned_by(&root.join("p/F.cs"), Some(&root.join("p/a.csproj"))));
        assert!(!scopes.owned_by(&root.join("p/F.cs"), Some(&root.join("p/b.csproj"))));
        // Each project still gets its own scope.
        assert_eq!(scopes.list().len(), 2);
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
