//! MOD004 sole-caller findings, shared by the Rust and C# rules.

use crate::reporting::Diagnostic;
use ahash::AHashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// MOD004 findings grouped by the file each finding anchors to.
///
/// Built once per run by each language's `analyze`; the pipeline then
/// asks for each file's share with [`SoleCallerFindings::for_file`].
pub(crate) struct SoleCallerFindings {
    /// Anchor file (the caller's file) -> its precomputed diagnostics.
    by_file: AHashMap<PathBuf, Vec<Diagnostic>>,
}

impl SoleCallerFindings {
    /// Wrap diagnostics already grouped by anchor file (the caller's file).
    pub(crate) fn new(by_file: AHashMap<PathBuf, Vec<Diagnostic>>) -> Self {
        Self { by_file }
    }

    /// Absorb `other`'s anchors into this set.
    ///
    /// Production merges come from disjoint analysis scopes (one crate
    /// or project closure per anchor file), so merge order does not
    /// matter. A shared anchor appends `other`'s findings after
    /// `self`'s.
    ///
    /// # Arguments
    ///
    /// - `other` - findings from one more analysis scope; consumed.
    pub(crate) fn merge(&mut self, other: Self) {
        for (file, mut findings) in other.by_file {
            self.by_file.entry(file).or_default().append(&mut findings);
        }
    }

    /// Drop findings anchored at files rejected by `keep`.
    ///
    /// Scopes overlap when one project references another; a scope
    /// keeps only the anchors it owns, so one file never anchors
    /// findings from two scopes.
    ///
    /// # Arguments
    ///
    /// - `keep` - receives each anchor file; return `true` to keep it.
    pub(crate) fn retain_anchors(&mut self, keep: impl Fn(&Path) -> bool) {
        self.by_file.retain(|file, _| keep(file));
    }

    /// The diagnostics anchored at `path`.
    ///
    /// Falls back to the canonicalized spelling of `path`: findings are
    /// keyed by canonicalized paths, while the pipeline may pass
    /// unresolved input paths.
    ///
    /// # Arguments
    ///
    /// - `path` - the file under lint, in any spelling.
    ///
    /// # Returns
    ///
    /// The findings anchored at that file; empty when it anchors none.
    pub(crate) fn for_file(&self, path: &Path) -> &[Diagnostic] {
        if let Some(found) = self.by_file.get(path) {
            return found;
        }
        let canonical = fs::canonicalize(path).ok();
        canonical
            .as_deref()
            .and_then(|p| self.by_file.get(p))
            .map_or(&[], Vec::as_slice)
    }

    /// Every diagnostic across all anchor files.
    ///
    /// # Returns
    ///
    /// All findings, in unspecified order.
    #[cfg(test)]
    pub(crate) fn all(&self) -> impl Iterator<Item = &Diagnostic> {
        self.by_file.values().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::SoleCallerFindings;
    use crate::reporting::{Diagnostic, Severity};
    use crate::rules::lint::CODE_MOD004;
    use ahash::AHashMap;
    use std::path::{Path, PathBuf};

    #[cfg(unix)]
    use std::fs;

    /// One MOD004 finding, tagged by name for assertions.
    fn finding(name: &str) -> Diagnostic {
        Diagnostic {
            severity: Severity::Hint,
            code: CODE_MOD004,
            title: None,
            message: String::new(), // unused by assertions; no capacity needed
            line: 1,
            item_kind: "mod".to_string(),
            item_name: Some(name.to_string()),
        }
    }

    /// Two anchor files: `a.rs` holds two findings, `b.rs` one.
    fn grouped() -> SoleCallerFindings {
        let mut by_file: AHashMap<PathBuf, Vec<Diagnostic>> = AHashMap::new();
        by_file.insert(PathBuf::from("a.rs"), vec![finding("one"), finding("two")]);
        by_file.insert(PathBuf::from("b.rs"), vec![finding("three")]);
        SoleCallerFindings::new(by_file)
    }

    // Core behavior.

    /// The exact anchor key returns that file's findings in order.
    #[test]
    fn for_file_should_return_anchored_diagnostics_when_key_matches() {
        // Arrange.
        let findings = grouped();

        // Act.
        let found = findings.for_file(&PathBuf::from("a.rs"));

        // Assert.
        let names: Vec<_> = found.iter().map(|d| d.item_name.as_deref()).collect();
        assert_eq!(names, [Some("one"), Some("two")]);
    }

    /// `all` flattens every anchor file's findings.
    #[test]
    fn all_should_yield_every_anchor_files_diagnostics() {
        // Arrange.
        let findings = grouped();

        // Act.
        let count = findings.all().count();

        // Assert.
        assert_eq!(count, 3);
    }

    /// Merging keeps each scope's anchors; a shared anchor appends.
    #[test]
    fn merge_should_combine_scopes_and_append_shared_anchors() {
        // Arrange: an empty accumulator, then one scope anchoring at
        // `b.rs`, then another anchoring at `a.rs` and the shared
        // `b.rs`.
        let mut accumulator = SoleCallerFindings::new(AHashMap::new());
        let mut second = AHashMap::new();
        second.insert(PathBuf::from("b.rs"), vec![finding("three")]);
        let mut first = AHashMap::new();
        first.insert(PathBuf::from("a.rs"), vec![finding("one")]);
        first.insert(PathBuf::from("b.rs"), vec![finding("four")]);

        // Act.
        accumulator.merge(SoleCallerFindings::new(second));
        accumulator.merge(SoleCallerFindings::new(first));

        // Assert.
        let names: Vec<_> = accumulator
            .for_file(&PathBuf::from("b.rs"))
            .iter()
            .map(|d| d.item_name.as_deref())
            .collect();
        assert_eq!(names, [Some("three"), Some("four")]);
        assert_eq!(accumulator.all().count(), 3);
    }

    // Edge cases.

    /// Only owned anchors survive; emptied anchors disappear.
    #[test]
    fn retain_anchors_should_drop_findings_when_anchor_is_not_owned() {
        // Arrange.
        let mut findings = grouped();

        // Act.
        findings.retain_anchors(|file| file != Path::new("a.rs"));

        // Assert.
        assert!(findings.for_file(&PathBuf::from("a.rs")).is_empty());
        assert_eq!(findings.all().count(), 1, "b.rs keeps its finding");
    }

    /// Unknown anchors anchor nothing.
    #[test]
    fn for_file_should_return_empty_when_file_anchors_nothing() {
        // Arrange.
        let findings = grouped();

        // Act and assert.
        assert!(findings.for_file(&PathBuf::from("other.rs")).is_empty());
    }

    /// `for_file` falls back to the `fs::canonicalize` result: real
    /// runs key findings by resolved absolute paths while inputs may
    /// arrive unresolved.
    #[cfg(unix)]
    #[test]
    fn for_file_should_fall_back_to_canonicalized_spelling_when_alias_passed() {
        // Arrange: one anchor file holding two findings, keyed by its
        // resolved absolute path.
        let dir = std::env::temp_dir().join(format!(
            "rust-llm-tidy-sole-caller-lookup-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let run = dir.join("run.rs");
        fs::write(&run, "//! Module docs.\n").unwrap();

        let canonical = fs::canonicalize(&run).unwrap();
        let mut by_file: AHashMap<PathBuf, Vec<Diagnostic>> = AHashMap::new();
        by_file.insert(canonical.clone(), vec![finding("one"), finding("two")]);
        let findings = SoleCallerFindings::new(by_file);

        // A symlink spells the file differently yet canonicalizes to
        // it (Path equality ignores `.` components, so an in-place
        // alias would not exercise the fallback).
        let aliased = dir.join("alias.rs");
        std::os::unix::fs::symlink(&run, &aliased).unwrap();

        // Act.
        let exact = findings.for_file(&canonical);
        let via_alias = findings.for_file(&aliased);

        // Assert: the alias sees the complete anchor result.
        let names: Vec<_> = exact.iter().map(|d| d.item_name.as_deref()).collect();
        assert_eq!(names, [Some("one"), Some("two")]);
        assert_eq!(via_alias, exact);
        assert_eq!(via_alias.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }
}
