//! Recognized documentation locations for the TEXT010 audience reminder.
//!
//! Detection is deliberately shallow: it classifies a file by its name and
//! ancestors, never by its content, and it parses no configuration.
//!
//! The marker table below is the single place listing where each
//! documentation tool keeps recognizable files or folders; add new tools
//! there.
//!
//! # Signals, in precedence order
//!
//! 1. A conventional filename stem - `README`, `QUICKSTART`, or
//!    `GETTING_STARTED` - with a prose extension.
//! 2. The nearest documentation-tool marker in an ancestor directory, up to
//!    the repository boundary.
//! 3. A `docs` directory component below that boundary.
//!
//! A signal only guesses the context: TEXT010 asks the reader to check the
//! audience rather than claiming a defect.
//!
//! The lint excludes `AGENTS.md` because it instructs agents rather than
//! users. It classifies only prose profiles (the markdown family), so
//! `docs/example.rs` never matches.
//!
//! # Bounds
//!
//! Each file inspects at most [`MAX_ANCESTOR_DEPTH`] ancestors. Cached
//! probe results are capped at [`MAX_CACHED_DIRECTORIES`].

use crate::languages::registry as langs;
use ahash::AHashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Agent-instruction filename excluded from documentation reminders.
const AGENT_INSTRUCTION_FILENAME: &str = "AGENTS.md";
/// Conventional documentation filename stems, matched case-insensitively.
const CONVENTIONAL_FILENAMES: &[&str] = &["README", "QUICKSTART", "GETTING_STARTED"];
/// Directory component treated as documentation evidence.
const DOCS_DIRECTORY: &str = "docs";
/// Documentation-tool markers probed in each ancestor directory, in order.
///
/// This table is the whole location registry: one row per tool and marker.
/// A marker is only a guess, so TEXT010 asks for an audience review instead
/// of enforcing a rule.
const DOCUMENTATION_MARKERS: &[DocumentationMarker] = &[
    // MkDocs
    DocumentationMarker::file("mkdocs.yml"),
    DocumentationMarker::file("mkdocs.yaml"),
    // Docusaurus
    DocumentationMarker::file("docusaurus.config.js"),
    DocumentationMarker::file("docusaurus.config.ts"),
    DocumentationMarker::file("docusaurus.config.mjs"),
    DocumentationMarker::file("docusaurus.config.cjs"),
    // VitePress
    DocumentationMarker::directory(".vitepress"),
    // VuePress
    DocumentationMarker::directory(".vuepress"),
    // mdBook
    DocumentationMarker::file("book.toml"),
    // Antora
    DocumentationMarker::file("antora.yml"),
    // Read the Docs
    DocumentationMarker::file(".readthedocs.yml"),
    DocumentationMarker::file(".readthedocs.yaml"),
];
/// Maximum ancestor directories inspected per file.
const MAX_ANCESTOR_DEPTH: usize = 64;
/// Maximum per-directory probe results cached per run.
const MAX_CACHED_DIRECTORIES: usize = 4096;

/// Per-directory probe results, cached across one run's classification.
#[derive(Clone, Copy, Debug)]
struct DirectoryFacts {
    /// First recognized marker entry found in this directory.
    marker: Option<&'static str>,
    /// Whether this directory is named `docs`.
    docs: bool,
    /// Whether this directory holds a `.git` entry (repository boundary).
    boundary: bool,
}

/// Documentation signals for one run's selected inputs.
///
/// Classify once before parallel lint execution; the lint phase reads these
/// facts and never walks the filesystem itself.
#[derive(Debug, Default)]
pub(crate) struct DocumentationContext {
    signals: AHashMap<PathBuf, DocumentationSignal>,
}

/// One documentation-tool filesystem marker.
struct DocumentationMarker {
    /// Exact directory-entry name probed in each ancestor directory.
    entry: &'static str,
    kind: MarkerKind,
}

/// Evidence that a selected file is likely documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocumentationSignal {
    /// A conventional documentation filename stem.
    Filename(&'static str),
    /// A documentation-tool marker in an ancestor directory.
    Marker(&'static str),
    /// A `docs` directory component below the repository boundary.
    DocsDirectory,
}

/// Whether a marker names a file or a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerKind {
    File,
    Directory,
}

impl DocumentationContext {
    /// Classify every selected path, keeping only detected files.
    pub(crate) fn build(paths: &[PathBuf]) -> Self {
        let mut signals = AHashMap::new();
        let mut directories = AHashMap::new();
        for path in paths {
            if let Some(signal) = signal_for(path, &mut directories) {
                signals.insert(path.clone(), signal);
            }
        }
        Self { signals }
    }

    /// The documentation signal recorded for `path`, if any.
    pub(crate) fn signal(&self, path: &Path) -> Option<DocumentationSignal> {
        self.signals.get(path).copied()
    }
}

impl DocumentationMarker {
    /// A marker file, such as `mkdocs.yml`.
    const fn file(entry: &'static str) -> Self {
        Self {
            entry,
            kind: MarkerKind::File,
        }
    }

    /// A marker directory, such as `.vitepress`.
    const fn directory(entry: &'static str) -> Self {
        Self {
            entry,
            kind: MarkerKind::Directory,
        }
    }

    /// Whether `dir` holds this marker.
    fn present_in(&self, dir: &Path) -> bool {
        let candidate = dir.join(self.entry);
        match self.kind {
            MarkerKind::File => candidate.is_file(),
            MarkerKind::Directory => candidate.is_dir(),
        }
    }
}

impl DocumentationSignal {
    /// Short evidence phrase for the reminder, without quoting file content.
    pub(crate) fn reason(self) -> String {
        match self {
            Self::Filename(stem) => format!("{stem} filename"),
            Self::Marker(entry) => format!("nearby {entry}"),
            Self::DocsDirectory => "file is under docs/".to_string(),
        }
    }
}

/// Classify one selected path; `None` when no documentation signal applies.
fn signal_for(
    path: &Path,
    cache: &mut AHashMap<PathBuf, DirectoryFacts>,
) -> Option<DocumentationSignal> {
    let name = path.file_name().and_then(OsStr::to_str)?;
    if name.eq_ignore_ascii_case(AGENT_INSTRUCTION_FILENAME) {
        return None;
    }
    let extension = path.extension().and_then(OsStr::to_str).unwrap_or("");
    if langs::profile_for(extension).text_lints != langs::TextLints::Prose {
        return None;
    }
    if let Some(stem) = conventional_stem(path) {
        return Some(DocumentationSignal::Filename(stem));
    }
    ancestry_signal(path, cache)
}

/// Nearest documentation evidence in the file's ancestors.
///
/// Walks from the file's directory to the first `.git` boundary, inclusive,
/// and never crosses it. A marker near the file wins over `docs/`.
fn ancestry_signal(
    path: &Path,
    cache: &mut AHashMap<PathBuf, DirectoryFacts>,
) -> Option<DocumentationSignal> {
    let anchored = traversal_path(path)?;
    let mut docs = false;
    let mut current = anchored.parent();
    for _ in 0..MAX_ANCESTOR_DEPTH {
        let Some(dir) = current else {
            break;
        };
        let facts = directory_facts(dir, cache);
        docs |= facts.docs;
        if let Some(entry) = facts.marker {
            return Some(DocumentationSignal::Marker(entry));
        }
        if facts.boundary {
            break;
        }
        current = dir.parent();
    }
    docs.then_some(DocumentationSignal::DocsDirectory)
}

/// Conventional stem for a documentation filename, matched case-insensitively.
fn conventional_stem(path: &Path) -> Option<&'static str> {
    let stem = path.file_stem().and_then(OsStr::to_str)?;
    CONVENTIONAL_FILENAMES
        .iter()
        .find(|candidate| stem.eq_ignore_ascii_case(candidate))
        .copied()
}

/// Probe `dir` once, reusing the result while the cache has room.
fn directory_facts(dir: &Path, cache: &mut AHashMap<PathBuf, DirectoryFacts>) -> DirectoryFacts {
    if let Some(facts) = cache.get(dir) {
        return *facts;
    }
    let facts = DirectoryFacts {
        marker: DOCUMENTATION_MARKERS
            .iter()
            .find(|marker| marker.present_in(dir))
            .map(|marker| marker.entry),
        docs: dir
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.eq_ignore_ascii_case(DOCS_DIRECTORY)),
        boundary: dir.join(".git").exists(),
    };
    if cache.len() < MAX_CACHED_DIRECTORIES {
        cache.insert(dir.to_path_buf(), facts);
    }
    facts
}

/// Absolute location to walk for `path`.
///
/// Input discovery keeps the caller's spelling, so a relative path would
/// otherwise stop its walk at the working directory. Absolute paths stay
/// unchanged, and the original path remains the signal key.
///
/// An unresolvable working directory fails closed for ancestry detection
/// only.
fn traversal_path(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    std::env::current_dir().ok().map(|cwd| cwd.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::slice::from_ref;
    use rstest::rstest;
    use std::fs;

    /// Classify `target` among `files`, written inside one temp root.
    ///
    /// `files` must contain `target`; any other entry only builds the tree.
    fn detect(files: &[(&str, &str)], target: &str) -> Option<DocumentationSignal> {
        let directory = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for (relative, content) in files {
            let path = directory.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            paths.push(path);
        }
        let context = DocumentationContext::build(&paths);

        context.signal(&directory.path().join(target))
    }

    // ── Filename and directory signals ──

    #[rstest]
    #[case::readme(&[("README.MD", "# Title\n")], "README.MD", Some(DocumentationSignal::Filename("README")))]
    #[case::readme_nested(
        &[("packages/widget/readme.markdown", "# Title\n")],
        "packages/widget/readme.markdown",
        Some(DocumentationSignal::Filename("README"))
    )]
    #[case::quickstart(&[("QUICKSTART.txt", "Start.\n")], "QUICKSTART.txt", Some(DocumentationSignal::Filename("QUICKSTART")))]
    #[case::getting_started(
        &[("GETTING_STARTED.mdx", "# Start\n")],
        "GETTING_STARTED.mdx",
        Some(DocumentationSignal::Filename("GETTING_STARTED"))
    )]
    #[case::similar_stem(&[("readme-helper.md", "# Help\n")], "readme-helper.md", None)]
    #[case::docs(&[("docs/setup.md", "# Setup\n")], "docs/setup.md", Some(DocumentationSignal::DocsDirectory))]
    #[case::docs_case(
        &[("Docs/Setup.MARKDOWN", "# Setup\n")],
        "Docs/Setup.MARKDOWN",
        Some(DocumentationSignal::DocsDirectory)
    )]
    #[case::docs_nested(
        &[("packages/widget/docs/reference.md", "# Reference\n")],
        "packages/widget/docs/reference.md",
        Some(DocumentationSignal::DocsDirectory)
    )]
    #[case::similar_directory(&[("mydocs/setup.md", "# Setup\n")], "mydocs/setup.md", None)]
    #[case::similar_file(&[("docs.md", "# Docs\n")], "docs.md", None)]
    fn classification_should_follow_names_and_components(
        #[case] files: &[(&str, &str)],
        #[case] target: &str,
        #[case] expected: Option<DocumentationSignal>,
    ) {
        assert_eq!(detect(files, target), expected);
    }

    #[rstest]
    #[case::agents(&[("docs/AGENTS.md", "# Rules\n")], "docs/AGENTS.md")]
    #[case::agents_case(&[("AGENTS.MD", "# Rules\n")], "AGENTS.MD")]
    fn agent_instructions_should_never_match(#[case] files: &[(&str, &str)], #[case] target: &str) {
        assert_eq!(detect(files, target), None);
    }

    #[rstest]
    #[case::rust(&[("docs/example.rs", "fn f() {}\n")], "docs/example.rs")]
    #[case::data(&[("docs/config.json", "{}\n")], "docs/config.json")]
    fn non_prose_files_should_never_match(#[case] files: &[(&str, &str)], #[case] target: &str) {
        assert_eq!(detect(files, target), None);
    }

    // ── Tool markers ──

    #[rstest]
    #[case::mkdocs_yml(
        &[("mkdocs.yml", "site_name: docs\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker("mkdocs.yml"))
    )]
    #[case::mkdocs_yaml(
        &[("mkdocs.yaml", "site_name: docs\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker("mkdocs.yaml"))
    )]
    #[case::docusaurus(
        &[("docusaurus.config.ts", "export default {};\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker("docusaurus.config.ts"))
    )]
    #[case::vitepress(
        &[(".vitepress/config.ts", "export default {};\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker(".vitepress"))
    )]
    #[case::vuepress(
        &[(".vuepress/config.js", "module.exports = {};\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker(".vuepress"))
    )]
    #[case::mdbook(
        &[("book.toml", "[book]\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker("book.toml"))
    )]
    #[case::antora(
        &[("antora.yml", "name: docs\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker("antora.yml"))
    )]
    #[case::readthedocs(
        &[(".readthedocs.yaml", "version: 2\n"), ("guide.md", "# Guide\n")],
        "guide.md",
        Some(DocumentationSignal::Marker(".readthedocs.yaml"))
    )]
    fn nearby_markers_should_classify_prose_files(
        #[case] files: &[(&str, &str)],
        #[case] target: &str,
        #[case] expected: Option<DocumentationSignal>,
    ) {
        assert_eq!(detect(files, target), expected);
    }

    /// A marker below the file's own directory is not an ancestor.
    #[test]
    fn markers_should_only_be_read_from_ancestors() {
        let files = &[
            ("sub/mkdocs.yml", "site_name: docs\n"),
            ("guide.md", "# Guide\n"),
        ];

        assert_eq!(detect(files, "guide.md"), None);
    }

    /// A nearer marker wins over a `docs/` component farther up.
    #[test]
    fn the_nearest_marker_should_take_precedence() {
        let files = &[
            ("repo/docs/site/book.toml", "[book]\n"),
            ("repo/docs/site/guide.md", "# Guide\n"),
        ];

        assert_eq!(
            detect(files, "repo/docs/site/guide.md"),
            Some(DocumentationSignal::Marker("book.toml"))
        );
    }

    /// The nearest marker also wins between two nested projects.
    #[test]
    fn a_nested_project_marker_should_shadow_an_outer_one() {
        let files = &[
            ("outer/mkdocs.yml", "site_name: outer\n"),
            ("outer/inner/book.toml", "[book]\n"),
            ("outer/inner/guide.md", "# Guide\n"),
        ];

        assert_eq!(
            detect(files, "outer/inner/guide.md"),
            Some(DocumentationSignal::Marker("book.toml"))
        );
    }

    // ── Repository boundary ──

    /// Markers above the nearest `.git` boundary stay invisible.
    #[test]
    fn discovery_should_stop_at_the_repository_boundary() {
        let files = &[
            ("outer/mkdocs.yml", "site_name: outer\n"),
            ("outer/inner/.git/HEAD", "ref: refs/heads/main\n"),
            ("outer/inner/guide.md", "# Guide\n"),
        ];

        assert_eq!(detect(files, "outer/inner/guide.md"), None);
    }

    /// The boundary directory itself still counts as part of the project.
    #[test]
    fn markers_should_be_read_from_the_boundary_directory() {
        let files = &[
            ("repo/mkdocs.yml", "site_name: docs\n"),
            ("repo/.git/HEAD", "ref: refs/heads/main\n"),
            ("repo/guide.md", "# Guide\n"),
        ];

        assert_eq!(
            detect(files, "repo/guide.md"),
            Some(DocumentationSignal::Marker("mkdocs.yml"))
        );
    }

    /// The ancestor walk stops at [`MAX_ANCESTOR_DEPTH`].
    #[rstest]
    #[case::at_limit(MAX_ANCESTOR_DEPTH - 1, Some(DocumentationSignal::Marker("mkdocs.yml")))]
    #[case::beyond_limit(MAX_ANCESTOR_DEPTH, None)]
    fn marker_discovery_should_stop_at_the_ancestor_limit(
        #[case] depth: usize,
        #[case] expected: Option<DocumentationSignal>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("mkdocs.yml"), "site_name: docs\n").unwrap();
        let mut nested = String::new();
        for index in 0..depth {
            nested.push_str(&format!("d{index}/"));
        }
        let path = directory.path().join(format!("{nested}guide.md"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "# Guide\n").unwrap();

        let context = DocumentationContext::build(from_ref(&path));

        assert_eq!(context.signal(&path), expected);
    }
}
