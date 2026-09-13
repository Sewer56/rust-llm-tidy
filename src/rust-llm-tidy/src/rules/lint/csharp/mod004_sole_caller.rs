//! `MOD004`: nest a C# namespace under its sole production caller.
//!
//! File layout that follows control flow reads top-down: when exactly
//! one namespace's code references another, the referenced namespace
//! can live beneath the caller.
//!
//! This rule measures that fan-in from the shared namespace
//! reference facts ([`NamespaceRefIndex`]) and emits one non-gating
//! hint per qualifying namespace.
//!
//! # How a namespace qualifies
//!
//! For each namespace `N` (the empty root namespace excluded):
//!
//! - Collect the reference edges whose target is `N` itself.
//! - Ignore edges from inside `N` (self-references and references
//!   from namespaces nested under `N`).
//! - Group the rest by caller unit: the caller's namespace truncated
//!   to `N`'s depth, mirroring the Rust rule. One namespace split
//!   across several files therefore counts once.
//! - `N` qualifies when exactly one caller unit remains, and that
//!   unit is not `N`'s parent (the namespace minus its last
//!   segment).
//!
//! A qualifying namespace stays silent when the pair is mutual: the
//! caller is itself referenced only by `N`.
//!
//! The finding anchors at the caller's first reference in the
//! deterministic edge order, so it surfaces in diffs that add the
//! dependency.
//!
//! # Blind spots
//!
//! Inherited from [`NamespaceRefIndex`]:
//!
//! - Aliases beyond `using static`, and `global using`.
//! - Reflection, string-built names, `nameof`, and `dynamic`.
//! - Extension calls through implicit imports, and source generators.
//! - Test projects whose members carry none of the recognized test
//!   attributes.
//!
//! [`NamespaceRefIndex`]:
//! crate::languages::csharp::analysis::namespace_refs::NamespaceRefIndex

use crate::languages::csharp::analysis::namespace_refs::{NamespaceEdge, NamespaceRefIndex};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::sole_caller::SoleCallerFindings;
use crate::rules::registry::CODE_MOD004;
use ahash::AHashMap;
use std::path::PathBuf;

/// Analyze namespace reference facts for sole-caller placement hints.
///
/// See the [module documentation] for the qualification rules.
/// Namespaces are visited in sorted segment order, so output is
/// deterministic.
///
/// [module documentation]: self
///
/// # Arguments
///
/// - `index` - the namespace facts over the C# project closure.
///
/// # Returns
///
/// The findings, grouped by anchor file (the caller's file).
pub(crate) fn analyze(index: &NamespaceRefIndex) -> SoleCallerFindings {
    let edges = index.edges();

    // Candidate namespaces: every distinct nonempty edge target,
    // sorted by segments so ancestors precede descendants.
    let mut namespaces: Vec<Vec<&str>> = edges
        .iter()
        .filter(|edge| !edge.target.is_empty())
        .map(|edge| edge.target.split('.').collect())
        .collect();
    namespaces.sort_unstable();
    namespaces.dedup();

    let mut by_file: AHashMap<PathBuf, Vec<Diagnostic>> = AHashMap::new();
    for namespace in namespaces {
        let Some((caller, first, count)) = sole_caller(&namespace, edges) else {
            continue;
        };
        // Parent-to-child references never flag.
        if caller.len() == namespace.len() - 1 && caller[..] == namespace[..caller.len()] {
            continue;
        }
        // Mutual sole-caller pairs stay silent.
        if sole_external_referencer_is(&caller, &namespace, edges) {
            continue;
        }
        by_file
            .entry(first.file.clone())
            .or_default()
            .push(diagnostic(&namespace, &caller, first, count));
    }
    SoleCallerFindings::new(by_file)
}

/// Build the MOD004 hint for one qualifying namespace.
fn diagnostic(
    namespace: &[&str],
    caller: &[&str],
    first: &NamespaceEdge,
    count: usize,
) -> Diagnostic {
    let last = namespace[namespace.len() - 1];
    let name = (*last).to_string();
    let namespace_path = namespace.join(".");
    let caller_path = caller.join(".");
    let nested = format!("{caller_path}.{last}");
    let references = if count == 1 {
        "1 reference".to_string()
    } else {
        format!("{count} references")
    };
    Diagnostic {
        severity: Severity::Hint,
        code: CODE_MOD004,
        title: Some("sole-caller namespace placement".into()),
        message: format!(
            "namespace `{namespace_path}` is referenced only by `{caller_path}` \
             ({references}).\n\
             Why:\n\
             - Nesting helpers under their callers lets readers follow call flow\n\
             through the file layout.\n\
             Suggestions:\n\
             - Consider nesting `{namespace_path}` as `{nested}`, with folders to match.\n\
             Update references and preserve behavior and any public API.\n\
             - Keep the current layout if reuse or readability favors it."
        ),
        line: first.line,
        item_kind: "namespace".to_string(),
        item_name: Some(name),
    }
}

/// The sole external caller of `namespace`, if there is exactly one.
///
/// The caller unit is the referencing namespace truncated to
/// `namespace`'s depth. One namespace split across files (or
/// referencing through a nested namespace) therefore counts once.
///
/// # Arguments
///
/// - `namespace` - the candidate namespace's segments, nonempty.
/// - `edges` - the closure's reference edges, in deterministic order.
///
/// # Returns
///
/// `Some((caller segments, first edge, reference count))` when
/// exactly one caller unit references the namespace from outside;
/// `None` otherwise.
fn sole_caller<'a>(
    namespace: &[&'a str],
    edges: &'a [NamespaceEdge],
) -> Option<(Vec<&'a str>, &'a NamespaceEdge, usize)> {
    let name = namespace.join(".");
    let mut caller: Option<Vec<&'a str>> = None;
    let mut first: Option<&'a NamespaceEdge> = None;
    let mut count = 0usize;
    for edge in edges {
        // Only references whose target is the namespace itself count.
        if edge.target.as_ref() != name {
            continue;
        }
        // References from inside the namespace count for no caller.
        let from = edge.from.as_ref();
        if is_within(from, namespace) {
            continue;
        }
        let unit: Vec<&'a str> = from.split('.').take(namespace.len()).collect();
        match &caller {
            None => caller = Some(unit),
            Some(seen) if *seen != unit => return None, // two caller units
            _ => {}
        }
        if first.is_none() {
            first = Some(edge);
        }
        count += 1;
    }
    Some((caller?, first?, count))
}

/// True when `caller`'s namespace has exactly one external referencer
/// and it is `namespace`'s unit at the caller's depth (a mutual
/// sole-caller pair).
fn sole_external_referencer_is(
    caller: &[&str],
    namespace: &[&str],
    edges: &[NamespaceEdge],
) -> bool {
    let name = caller.join(".");
    let mut referencer: Option<Vec<&str>> = None;
    for edge in edges {
        if edge.target.as_ref() != name {
            continue;
        }
        if is_within(edge.from.as_ref(), caller) {
            continue;
        }
        let unit: Vec<&str> = edge.from.as_ref().split('.').take(caller.len()).collect();
        match &referencer {
            None => referencer = Some(unit),
            Some(seen) if *seen != unit => return false,
            _ => {}
        }
    }
    let namespace_unit: Vec<&str> = namespace.iter().take(caller.len()).copied().collect();
    referencer.is_some_and(|seen| seen == namespace_unit)
}

/// True when `name` is `namespace` or nested beneath it.
fn is_within(name: &str, namespace: &[&str]) -> bool {
    let mut segments = name.split('.');
    for expected in namespace {
        if segments.next() != Some(*expected) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::analyze;
    use crate::languages::csharp::analysis::namespace_refs::NamespaceRefIndex;
    use crate::languages::csharp::parse::parse;
    use crate::reporting::Severity;
    use crate::rules::lint::CODE_MOD004;
    use crate::rules::lint::sole_caller::SoleCallerFindings;
    use std::path::Path;

    /// Build an index over inline `(path, source)` fixtures, then analyze.
    fn analyze_sources(files: &[(&str, &str)]) -> SoleCallerFindings {
        let parses: Vec<_> = files
            .iter()
            .map(|(path, source)| (*path, parse(source).expect("fixture parses")))
            .collect();
        let index = NamespaceRefIndex::from_parses(
            parses
                .iter()
                .map(|(path, parsed)| (Path::new(path), parsed)),
        );
        analyze(&index)
    }

    /// Every MOD004 finding across all anchor files, sorted by line.
    fn all_findings(findings: &SoleCallerFindings) -> Vec<&crate::reporting::Diagnostic> {
        let mut found: Vec<_> = findings.all().collect();
        found.sort_by_key(|d| d.line);
        found
    }

    /// The declaring fixture most tests reference.
    const CORE_WIDGET: &str = "namespace App.Core\n{\n    class Widget { }\n}\n";

    /// The sibling-caller fixture referencing `App.Core`.
    const RUN_CALLER: &str = r#"namespace App.Run
{
    using App.Core;

    class Runner
    {
        Widget value;
    }
}
"#;

    // Core behavior: the qualifying cases.

    /// A sole sibling caller flags at the caller's file on the first
    /// referencing line.
    #[test]
    fn analyze_should_flag_namespace_when_sibling_is_sole_caller() {
        // Arrange.
        let sources = [("lib.cs", CORE_WIDGET), ("run.cs", RUN_CALLER)];

        // Act.
        let findings = analyze_sources(&sources);

        // Assert.
        let found = findings.for_file(Path::new("run.cs"));
        assert_eq!(found.len(), 1, "one finding at the caller's file");
        let d = &found[0];
        assert_eq!(d.code, CODE_MOD004);
        assert_eq!(d.severity, Severity::Hint);
        assert_eq!(d.line, 7, "anchored at the first referencing line");
        assert_eq!(d.item_kind, "namespace");
        assert_eq!(d.item_name.as_deref(), Some("Core"));
        assert_eq!(d.title(), "sole-caller namespace placement");
        assert_eq!(
            d.message,
            "namespace `App.Core` is referenced only by `App.Run` (1 reference).\n\
             Why:\n\
             - Nesting helpers under their callers lets readers follow call flow\n\
             through the file layout.\n\
             Suggestions:\n\
             - Consider nesting `App.Core` as `App.Run.Core`, with folders to match.\n\
             Update references and preserve behavior and any public API.\n\
             - Keep the current layout if reuse or readability favors it."
        );
    }

    /// A caller namespace split across two files yields exactly one
    /// finding, anchored at the namespace's first edge in edge order.
    #[test]
    fn analyze_should_emit_one_finding_when_caller_spans_two_files() {
        let findings = analyze_sources(&[
            ("lib.cs", CORE_WIDGET),
            ("run1.cs", RUN_CALLER),
            ("run2.cs", RUN_CALLER),
        ]);

        let found = all_findings(&findings);
        assert_eq!(found.len(), 1, "caller aggregates by namespace");
        assert!(
            found[0].message.contains("2 references"),
            "{}",
            found[0].message
        );
        // Both references aggregate into one finding under one anchor.
        let at_run1 = findings.for_file(Path::new("run1.cs"));
        let at_run2 = findings.for_file(Path::new("run2.cs"));
        assert_eq!(at_run1.len() + at_run2.len(), 1, "one anchor file only");
    }

    // Edge cases: the silent cases.

    #[test]
    fn analyze_should_stay_silent_when_caller_is_parent_namespace() {
        let findings = analyze_sources(&[(
            "lib.cs",
            r#"namespace App
{
    using App.Core;

    class Host
    {
        Widget value;
    }
}

namespace App.Core
{
    class Widget { }
}
"#,
        )]);

        assert!(
            all_findings(&findings).is_empty(),
            "parent references never flag"
        );
    }

    #[test]
    fn analyze_should_stay_silent_when_two_caller_namespaces_exist() {
        let other = RUN_CALLER.replace("App.Run", "App.Codec");
        let findings = analyze_sources(&[
            ("lib.cs", CORE_WIDGET),
            ("run.cs", RUN_CALLER),
            ("codec.cs", &other),
        ]);

        assert!(
            all_findings(&findings).is_empty(),
            "shared namespaces stay silent"
        );
    }

    #[test]
    fn analyze_should_stay_silent_when_pair_is_mutual() {
        let findings = analyze_sources(&[(
            "lib.cs",
            r#"namespace App.A
{
    using App.B;

    class One
    {
        Thing value;
    }
}

namespace App.B
{
    using App.A;

    class Thing
    {
        One value;
    }
}
"#,
        )]);

        assert!(
            all_findings(&findings).is_empty(),
            "mutual pairs stay silent"
        );
    }

    /// The empty root namespace never qualifies: a type outside any
    /// namespace referenced from a named namespace stays silent.
    #[test]
    fn analyze_should_stay_silent_when_target_is_root_namespace() {
        let findings = analyze_sources(&[(
            "lib.cs",
            r#"class Loose { }

namespace App.Run
{
    class Runner
    {
        Loose value;
    }
}
"#,
        )]);

        assert!(all_findings(&findings).is_empty());
    }

    // Convenience: per-file lookup.

    /// Findings anchor only at the caller's file, never the callee's.
    #[test]
    fn for_file_should_return_only_that_files_findings() {
        let findings = analyze_sources(&[("lib.cs", CORE_WIDGET), ("run.cs", RUN_CALLER)]);

        assert!(!findings.for_file(Path::new("run.cs")).is_empty());
        assert!(
            findings.for_file(Path::new("lib.cs")).is_empty(),
            "the callee anchors nothing"
        );
        assert!(
            findings.for_file(Path::new("other.cs")).is_empty(),
            "unrelated files anchor nothing"
        );
    }
}
