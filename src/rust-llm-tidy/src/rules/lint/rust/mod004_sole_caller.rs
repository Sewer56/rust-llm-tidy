//! `MOD004`: suggest moving a module under its only production caller.
//!
//! When only one part of a crate uses a module, placing that module
//! beneath its caller makes the file layout easier to follow.
//!
//! This rule uses a whole-crate parse ([`RustCrateIndex`]) to find these
//! opportunities: one parse per crate owning an input, each analyzed
//! alone. Each suggestion is a hint, not a failing check.
//!
//! # When the rule suggests a move
//!
//! The rule checks every module except the crate root:
//!
//! - Find references to the module or any of its descendants.
//! - Ignore references from within that same subtree.
//! - Group the remaining callers by module subtree, at the depth of
//!   the module under review. A caller spread across several files,
//!   including nested modules, counts as one caller.
//! - Suggest a move if exactly one caller remains, unless the module
//!   already lives directly under that caller.
//!
//! Two modules that are each other's only caller get no suggestion:
//! neither is a clear choice to contain the other.
//!
//! If a module gets a suggestion, its descendants get no separate
//! suggestions. Moving the outer module already covers them.
//!
//! The hint points to the caller's first reference, so it can appear
//! in a diff that introduces the dependency.
//!
//! # Limitations
//!
//! [`RustCrateIndex`] cannot reliably track references through
//! re-exports, dynamic or trait dispatch, or paths inside macro token
//! trees.
//!
//! It also skips `use` prefixes with only one segment: `use a::{..}`
//! is not resolved, but `use crate::a::{..}` is.
//!
//! The rule analyzes each crate alone: cross-crate callers are
//! invisible, so a module shared with a dependent crate can still
//! look sole-called.
//!
//! [`RustCrateIndex`]: crate::project::rust_crate::RustCrateIndex

use crate::project::rust_crate::{ReferenceEdge, RustCrateIndex};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::sole_caller::SoleCallerFindings;
use crate::rules::registry::CODE_MOD004;
use ahash::AHashMap;
use std::path::{Path, PathBuf};

/// Analyze one crate parse for sole-caller module placement hints.
///
/// See the [module documentation] for the qualification rules.
/// Modules are visited in sorted segment order and only the outermost
/// qualifying subtree emits, so output is deterministic.
///
/// [module documentation]: self
///
/// # Arguments
///
/// - `index` - the crate-level facts from one whole-crate parse.
///
/// # Returns
///
/// The findings, grouped by anchor file (the caller's file).
pub(crate) fn analyze(index: &RustCrateIndex) -> SoleCallerFindings {
    let edges = index.edges();

    // Module-path segments for every edge endpoint, resolved once.
    let mut segments: AHashMap<&Path, &[Box<str>]> = AHashMap::new();
    for edge in edges {
        for file in [&edge.from, &edge.target] {
            if !segments.contains_key(file.as_path())
                && let Some(segs) = index.segments_for(file)
            {
                segments.insert(file.as_path(), segs);
            }
        }
    }

    // Candidate modules: every nonempty prefix of a referenced target's path.
    //
    // Sort ancestors before descendants so the subtree filter below
    // only needs prefixes seen so far.
    // Vec::new(): the prefix count depends on the edges; no known bound.
    let mut modules: Vec<&[Box<str>]> = Vec::new();
    for edge in edges {
        if let Some(target) = segments.get(edge.target.as_path()) {
            for depth in 1..=target.len() {
                modules.push(&target[..depth]);
            }
        }
    }
    modules.sort_unstable();
    modules.dedup();

    let mut by_file: AHashMap<PathBuf, Vec<Diagnostic>> = AHashMap::new();
    // Emitted module paths: a module nested inside an emitted subtree is
    // covered by that finding and stays silent. No known emitted-count bound.
    let mut emitted: Vec<&[Box<str>]> = Vec::new();
    for module in modules {
        let Some((caller, first, count)) = sole_caller(module, &segments, edges) else {
            continue;
        };
        // Parent-to-child references never flag.
        if caller == &module[..module.len() - 1] {
            continue;
        }
        // Mutual sole-caller pairs stay silent.
        if sole_external_referencer_is(caller, module, &segments, edges) {
            continue;
        }
        // An emitted ancestor subtree already covers this module.
        if emitted.iter().any(|done| module.starts_with(done)) {
            continue;
        }
        emitted.push(module);
        by_file
            .entry(first.from.clone())
            .or_default()
            .push(diagnostic(module, caller, first, count));
    }
    SoleCallerFindings::new(by_file)
}

/// Build the MOD004 hint for one qualifying module.
fn diagnostic(
    module: &[Box<str>],
    caller: &[Box<str>],
    first: &ReferenceEdge,
    count: usize,
) -> Diagnostic {
    let name = module[module.len() - 1].to_string();
    let module_path = crate_path(module);
    let caller_path = crate_path(caller);
    let nested = format!("{caller_path}::{name}");
    let references = if count == 1 {
        "1 reference".to_string()
    } else {
        format!("{count} references")
    };
    Diagnostic {
        severity: Severity::Hint,
        code: CODE_MOD004,
        title: Some("sole-caller module placement".into()),
        message: format!(
            "module `{module_path}` is referenced only by `{caller_path}` \
             ({references}).\n\
             Why:\n\
             - Nesting code under its caller makes call flow easier to follow.\n\
             Suggestions:\n\
             - Consider moving `{name}` to `{nested}`.\n\
             Update references; preserve behavior and public APIs.\n\
             - Keep the current layout if it better supports reuse or readability."
        ),
        line: first.line,
        item_kind: "mod".to_string(),
        item_name: Some(name),
    }
}

/// The sole external caller of `module`'s subtree, if there is exactly one.
///
/// The caller unit is the subtree at `module`'s depth containing the
/// referencing file. One caller module split across files (or
/// referencing through a nested module) therefore counts once.
///
/// # Arguments
///
/// - `module` - the candidate module's path segments, nonempty.
/// - `segments` - resolved module paths for every edge endpoint.
/// - `edges` - the crate's reference edges, in deterministic order.
///
/// # Returns
///
/// `Some((caller, first edge, reference count))` when exactly one
/// caller unit references the subtree from outside; `None` otherwise.
fn sole_caller<'a>(
    module: &[Box<str>],
    segments: &AHashMap<&'a Path, &'a [Box<str>]>,
    edges: &'a [ReferenceEdge],
) -> Option<(&'a [Box<str>], &'a ReferenceEdge, usize)> {
    let mut caller: Option<&'a [Box<str>]> = None;
    let mut first: Option<&'a ReferenceEdge> = None;
    let mut count = 0usize;
    for edge in edges {
        // Only references whose target lies inside the subtree count.
        let Some(target) = segments.get(edge.target.as_path()) else {
            continue;
        };
        if !target.starts_with(module) {
            continue;
        }
        let Some(from) = segments.get(edge.from.as_path()) else {
            continue;
        };
        // Internal and self-references from inside the subtree count
        // for no caller.
        if from.starts_with(module) {
            continue;
        }
        let unit = &from[..module.len().min(from.len())];
        match caller {
            None => caller = Some(unit),
            Some(seen) if seen != unit => return None, // two caller units
            _ => {}
        }
        if first.is_none() {
            first = Some(edge);
        }
        count += 1;
    }
    Some((caller?, first?, count))
}

/// True when `caller`'s subtree has exactly one external referencer and it
/// is `module`'s unit at the caller's depth (a mutual sole-caller pair).
fn sole_external_referencer_is(
    caller: &[Box<str>],
    module: &[Box<str>],
    segments: &AHashMap<&Path, &[Box<str>]>,
    edges: &[ReferenceEdge],
) -> bool {
    let mut referencer: Option<&[Box<str>]> = None;
    for edge in edges {
        let Some(target) = segments.get(edge.target.as_path()) else {
            continue;
        };
        if !target.starts_with(caller) {
            continue;
        }
        let Some(from) = segments.get(edge.from.as_path()) else {
            continue;
        };
        if from.starts_with(caller) {
            continue;
        }
        let unit = &from[..caller.len().min(from.len())];
        match referencer {
            None => referencer = Some(unit),
            Some(seen) if seen != unit => return false,
            _ => {}
        }
    }
    let module_unit = &module[..caller.len().min(module.len())];
    referencer == Some(module_unit)
}

/// The `crate::`-joined spelling of a module path; the root is `crate`.
fn crate_path(segments: &[Box<str>]) -> String {
    if segments.is_empty() {
        "crate".to_string()
    } else {
        format!("crate::{}", segments.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use super::analyze;
    use crate::project::rust_crate::RustCrateIndex;
    use crate::reporting::Severity;
    use crate::rules::lint::CODE_MOD004;
    use crate::rules::lint::sole_caller::SoleCallerFindings;
    use crate::rules::transform::visibility::rust::ParsedFile;
    use std::path::{Path, PathBuf};

    /// Parse `(path, source)` pairs into [`ParsedFile`]s (discovery
    /// order = slice order).
    fn parse_files(sources: Vec<(PathBuf, String)>) -> Vec<ParsedFile> {
        sources
            .into_iter()
            .map(|(p, s)| ParsedFile::new(p, s).expect("test source must parse"))
            .collect()
    }

    /// Analyze a synthetic crate rooted at `src/lib.rs`.
    fn analyze_sources(sources: Vec<(PathBuf, String)>) -> SoleCallerFindings {
        let files = parse_files(sources);
        let index =
            RustCrateIndex::from_parsed(&PathBuf::from("src/lib.rs"), &files).expect("index");
        analyze(&index)
    }

    /// Every MOD004 finding across all anchor files, sorted by line.
    fn all_findings_sorted_by_line(
        findings: &SoleCallerFindings,
    ) -> Vec<&crate::reporting::Diagnostic> {
        let mut found: Vec<_> = findings.all().collect();
        found.sort_by_key(|d| d.line);
        found
    }

    fn src(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    // Core behavior: the qualifying cases.

    /// A sole sibling caller flags at the caller's file on the first
    /// referencing line.
    #[test]
    fn analyze_should_flag_module_when_sibling_is_sole_caller() {
        // Arrange.
        let sources = vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                src("src/load/mod.rs"),
                "pub fn go() {\n    crate::xbe::parse_xbe_header();\n    \
                 crate::xbe::Header;\n}\n"
                    .into(),
            ),
            (
                src("src/xbe.rs"),
                "pub fn parse_xbe_header() {}\npub struct Header;\n".into(),
            ),
        ];

        // Act.
        let findings = analyze_sources(sources);

        // Assert.
        let found = findings.for_file(Path::new("src/load/mod.rs"));
        assert_eq!(found.len(), 1, "one finding at the caller's file");
        let d = &found[0];
        assert_eq!(d.code, CODE_MOD004);
        assert_eq!(d.severity, Severity::Hint);
        assert_eq!(d.line, 2, "anchored at the first referencing line");
        assert_eq!(d.item_kind, "mod");
        assert_eq!(d.item_name.as_deref(), Some("xbe"));
        assert_eq!(d.title(), "sole-caller module placement");
        assert_eq!(
            d.message,
            "module `crate::xbe` is referenced only by `crate::load` (2 references).\n\
             Why:\n\
             - Nesting code under its caller makes call flow easier to follow.\n\
             Suggestions:\n\
             - Consider moving `xbe` to `crate::load::xbe`.\n\
             Update references; preserve behavior and public APIs.\n\
             - Keep the current layout if it better supports reuse or readability."
        );
    }

    /// A reference into the module's child counts for the module, and the
    /// covered child subtree stays silent.
    #[test]
    fn analyze_should_flag_module_when_reference_targets_its_child() {
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                src("src/load/mod.rs"),
                "pub fn go() { crate::xbe::hdr::read(); }\n".into(),
            ),
            (src("src/xbe/mod.rs"), "pub mod hdr;\n".into()),
            (src("src/xbe/hdr.rs"), "pub fn read() {}\n".into()),
        ]);

        let found = all_findings_sorted_by_line(&findings);
        assert_eq!(found.len(), 1, "only the outermost subtree emits");
        assert_eq!(found[0].item_name.as_deref(), Some("xbe"));
    }

    /// An ancestor-above-parent caller flags: the root reaching past its
    /// child.
    #[test]
    fn analyze_should_flag_module_when_caller_sits_above_parent() {
        let findings = analyze_sources(vec![
            (
                src("src/lib.rs"),
                "mod a;\npub fn go() { crate::a::b::f(); }\n".into(),
            ),
            (src("src/a/mod.rs"), "mod b;\n".into()),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);

        let found = findings.for_file(Path::new("src/lib.rs"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item_name.as_deref(), Some("b"));
        assert_eq!(found[0].line, 2);
    }

    /// A caller module split across files yields exactly one finding,
    /// anchored at its first edge in edge order.
    #[test]
    fn analyze_should_emit_one_finding_when_caller_spans_two_files() {
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                // Discovery order: mod.rs first, so its edge anchors.
                src("src/load/mod.rs"),
                "mod util;\npub fn go() { crate::xbe::f(); }\n".into(),
            ),
            (
                src("src/load/util.rs"),
                "pub fn h() { crate::xbe::f(); }\n".into(),
            ),
            (src("src/xbe.rs"), "pub fn f() {}\n".into()),
        ]);

        let found = findings.for_file(Path::new("src/load/mod.rs"));
        assert_eq!(found.len(), 1, "caller aggregates by module unit");
        assert_eq!(found[0].line, 2, "first edge in edge order anchors");
        assert!(
            found[0].message.contains("2 references"),
            "{}",
            found[0].message
        );
        assert!(
            findings.for_file(Path::new("src/load/util.rs")).is_empty(),
            "the second file anchors nothing"
        );
    }

    /// One anchor file keeps its findings in sorted module order even
    /// when the references arrive in a different order.
    #[test]
    fn analyze_should_keep_module_order_when_anchor_holds_two_findings() {
        // Arrange: `go` references `b` before `a`, so reference order
        // differs from the sorted module order.
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod load;\nmod a;\nmod b;\n".into()),
            (
                src("src/load/mod.rs"),
                "pub fn go() { crate::b::f(); crate::a::f(); }\n".into(),
            ),
            (src("src/a.rs"), "pub fn f() {}\n".into()),
            (src("src/b.rs"), "pub fn f() {}\n".into()),
        ]);

        // Act.
        let found = findings.for_file(Path::new("src/load/mod.rs"));

        // Assert.
        let names: Vec<_> = found.iter().map(|d| d.item_name.as_deref()).collect();
        assert_eq!(names, [Some("a"), Some("b")]);
    }

    // Edge cases: the silent cases.

    #[test]
    fn analyze_should_stay_silent_when_caller_is_parent_module() {
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod a;\n".into()),
            (
                src("src/a/mod.rs"),
                "mod b;\npub fn go() { crate::a::b::f(); }\n".into(),
            ),
            (src("src/a/b.rs"), "pub fn f() {}\n".into()),
        ]);

        assert!(
            all_findings_sorted_by_line(&findings).is_empty(),
            "parent references never flag"
        );
    }

    #[test]
    fn analyze_should_stay_silent_when_caller_is_root_of_top_level_module() {
        // The root is the parent of every top-level module, so a root-only
        // caller stays silent (the root module itself is never analyzed).
        let findings = analyze_sources(vec![
            (
                src("src/lib.rs"),
                "mod a;\npub fn go() { crate::a::f(); }\n".into(),
            ),
            (src("src/a.rs"), "pub fn f() {}\n".into()),
        ]);

        assert!(all_findings_sorted_by_line(&findings).is_empty());
    }

    #[test]
    fn analyze_should_stay_silent_when_two_caller_modules_exist() {
        let findings = analyze_sources(vec![
            (
                src("src/lib.rs"),
                "mod load;\nmod codec;\nmod util;\n".into(),
            ),
            (
                src("src/load/mod.rs"),
                "pub fn go() { crate::util::f(); }\n".into(),
            ),
            (
                src("src/codec/mod.rs"),
                "pub fn c() { crate::util::f(); }\n".into(),
            ),
            (src("src/util/mod.rs"), "pub fn f() {}\n".into()),
        ]);

        assert!(
            all_findings_sorted_by_line(&findings).is_empty(),
            "shared subtrees stay silent"
        );
    }

    #[test]
    fn analyze_should_stay_silent_when_pair_is_mutual() {
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod a;\nmod b;\n".into()),
            (
                src("src/a/mod.rs"),
                "pub fn f() { crate::b::g(); }\n".into(),
            ),
            (
                src("src/b/mod.rs"),
                "pub fn g() { crate::a::f(); }\n".into(),
            ),
        ]);

        assert!(
            all_findings_sorted_by_line(&findings).is_empty(),
            "mutual pairs stay silent"
        );
    }

    /// The mutual pair stays silent even when the reciprocal reference
    /// comes from the caller's non-root file.
    #[test]
    fn analyze_should_stay_silent_when_mutual_reference_comes_from_nested_file() {
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod a;\nmod b;\n".into()),
            (src("src/a/mod.rs"), "mod inner;\npub fn f() {}\n".into()),
            (
                src("src/a/inner.rs"),
                "pub fn h() { crate::b::g(); }\n".into(),
            ),
            (
                src("src/b/mod.rs"),
                "pub fn g() { crate::a::f(); }\n".into(),
            ),
        ]);

        // `a`'s subtree (mod.rs + inner.rs) is `b`'s sole caller unit, and
        // `b` is `a`'s sole external referencer: a mutual pair.
        assert!(all_findings_sorted_by_line(&findings).is_empty());
    }

    /// A self-reference from the module's own file counts for no caller.
    #[test]
    fn analyze_should_stay_silent_when_reference_is_self_reference() {
        let findings = analyze_sources(vec![
            (src("src/lib.rs"), "mod load;\nmod xbe;\n".into()),
            (
                src("src/load/mod.rs"),
                "pub fn go() { crate::xbe::f(); }\n".into(),
            ),
            (
                src("src/xbe.rs"),
                "pub fn f() {}\npub fn g() { crate::xbe::f(); }\n".into(),
            ),
        ]);

        let found = all_findings_sorted_by_line(&findings);
        assert_eq!(found.len(), 1, "load remains the sole caller");
        assert_eq!(found[0].line, 1);
    }

    // Convenience: per-file lookup.

    /// Two independent findings anchor at their own caller files only.
    #[test]
    fn for_file_should_return_only_that_files_findings() {
        let findings = analyze_sources(vec![
            (
                src("src/lib.rs"),
                "mod load;\nmod xbe;\nmod codec;\nmod util;\n".into(),
            ),
            (
                src("src/load/mod.rs"),
                "pub fn go() { crate::xbe::f(); }\n".into(),
            ),
            (
                src("src/codec/mod.rs"),
                "pub fn c() { crate::util::f(); }\n".into(),
            ),
            (src("src/xbe.rs"), "pub fn f() {}\n".into()),
            (src("src/util/mod.rs"), "pub fn f() {}\n".into()),
        ]);

        let at_load = findings.for_file(Path::new("src/load/mod.rs"));
        assert_eq!(at_load.len(), 1);
        assert_eq!(at_load[0].item_name.as_deref(), Some("xbe"));

        let at_codec = findings.for_file(Path::new("src/codec/mod.rs"));
        assert_eq!(at_codec.len(), 1);
        assert_eq!(at_codec[0].item_name.as_deref(), Some("util"));

        assert!(findings.for_file(Path::new("src/xbe.rs")).is_empty());
        assert!(findings.for_file(Path::new("src/lib.rs")).is_empty());
    }
}
