//! Shared benchmark fixtures and setup.
//!
//! Fixtures live in per-operation filesystem folders so each benchmark
//! iterates only the files relevant to its operation:
//!
//! - `fixtures/lint/`: named `<size>_<clean|dirty>` by lint outcome.
//! - `fixtures/reorder/`: named `<size>_<stable|dirty>` by reorder outcome
//!   (`stable` = already in canonical order, so reorder is a no-op).
//! - `fixtures/vis/`: `crate_small_lib`, `crate_small_foo`,
//!   `crate_medium_lib`, `crate_medium_foo`, `crate_medium_bar` - multi-file
//!   crate fixtures for the crate-aware vis bench.
//! - `fixtures/fences/`: named `<size>_<clean|dirty>` (plus `doc_*`) by fence
//!   outcome (`clean` = no nested same-marker fences, a borrowed no-op).
//! - `fixtures/links/`: named `<size>_<clean|dirty>` (plus `doc_*`) by link
//!   outcome (`clean` = no inline link repeated 2+ times, a borrowed no-op).
//!
//! Each fixture is a real `.rs` file from an open-source project, embedded
//! verbatim with [`include_str!`] (byte-exact copies, so the benchmarks
//! reflect realistic parse characteristics). Provenance (repo, path, pinned
//! permalink) is documented in the header comment of each fixture file.

use rust_llm_tidy_vis::{
    ModuleTree, ParsedFile, ReexportSet, build_module_tree, collect_crate_reexports,
};

/// Multi-file crate fixtures for the crate-aware vis bench: each entry is a
/// small crate as `(name, root_relative_path, [(path, source)])`. The parent
/// declares `pub(crate) mod foo;`; `foo.rs` holds bare-`pub` children that the
/// crate-aware pass narrows cross-file.
///
/// Sources are embedded inline (no filesystem I/O at bench time): the resolver
/// API `build_module_tree(root, &[ParsedFile])` accepts in-memory parsed files.
#[allow(dead_code)]
pub const CRATE_FIXTURES: &[(&str, &[(&str, &str)])] = &[
    (
        "small/crate-aware",
        &[
            (
                "src/lib.rs",
                include_str!("fixtures/vis/crate_small_lib.rs"),
            ),
            (
                "src/foo.rs",
                include_str!("fixtures/vis/crate_small_foo.rs"),
            ),
        ],
    ),
    (
        "medium/crate-aware",
        &[
            (
                "src/lib.rs",
                include_str!("fixtures/vis/crate_medium_lib.rs"),
            ),
            (
                "src/foo.rs",
                include_str!("fixtures/vis/crate_medium_foo.rs"),
            ),
            (
                "src/bar.rs",
                include_str!("fixtures/vis/crate_medium_bar.rs"),
            ),
        ],
    ),
];
/// Fence-fix benchmark fixtures: `(name, source)` pairs, named by size tier and
/// fence state.
///
/// `clean` fixtures contain backtick/tilde runs but no nested same-marker
/// fences, so [`fix_fences`] returns the input borrowed (a no-op). `dirty`
/// fixtures contain nested same-marker fences that get rewritten to alternate
/// markers. The `doc/*` variants are Rust source with `///` doc-comment fences,
/// exercising the doc-prefix stripping path.
///
/// [`fix_fences`]: rust_llm_tidy_fix::fix_fences
#[allow(dead_code)] // each bench compiles `common` separately, using one set
pub const FENCE_FIXTURES: &[(&str, &str)] = &[
    (
        "small/clean",
        include_str!("fixtures/fences/small_clean.md"),
    ),
    (
        "small/dirty",
        include_str!("fixtures/fences/small_dirty.md"),
    ),
    (
        "medium/clean",
        include_str!("fixtures/fences/medium_clean.md"),
    ),
    (
        "medium/dirty",
        include_str!("fixtures/fences/medium_dirty.md"),
    ),
    (
        "large/clean",
        include_str!("fixtures/fences/large_clean.md"),
    ),
    (
        "large/dirty",
        include_str!("fixtures/fences/large_dirty.md"),
    ),
    ("doc/clean", include_str!("fixtures/fences/doc_clean.rs")),
    ("doc/dirty", include_str!("fixtures/fences/doc_dirty.rs")),
];
/// Link-hoist benchmark fixtures: `(name, source)` pairs, named by size tier and
/// link state.
///
/// `clean` fixtures contain inline links but none repeated 2+ times, so
/// [`fix_links`] returns the input borrowed (a no-op that still exercises the
/// tally pass). `dirty` fixtures contain at least one `[text](url)` pair seen
/// 2+ times, which is rewritten to `[text]` plus an appended `[text]: url`
/// definition. The `doc/*` variants are Rust source with `///`/`//!`
/// inline links, exercising the doc-prefix stripping path.
///
/// [`fix_links`]: rust_llm_tidy_fix::fix_links
#[allow(dead_code)] // each bench compiles `common` separately, using one set
pub const LINK_FIXTURES: &[(&str, &str)] = &[
    ("small/clean", include_str!("fixtures/links/small_clean.md")),
    ("small/dirty", include_str!("fixtures/links/small_dirty.md")),
    (
        "medium/clean",
        include_str!("fixtures/links/medium_clean.md"),
    ),
    (
        "medium/dirty",
        include_str!("fixtures/links/medium_dirty.md"),
    ),
    ("large/clean", include_str!("fixtures/links/large_clean.md")),
    ("large/dirty", include_str!("fixtures/links/large_dirty.md")),
    ("doc/clean", include_str!("fixtures/links/doc_clean.rs")),
    ("doc/dirty", include_str!("fixtures/links/doc_dirty.rs")),
];
/// Lint benchmark fixtures: `(name, source)` pairs, named by lint state.
///
/// `clean` fixtures produce zero lint findings; `dirty` fixtures produce
/// several. Spans the three size tiers.
#[allow(dead_code)] // each bench compiles `common` separately, using one set
pub const LINT_FIXTURES: &[(&str, &str)] = &[
    ("small/clean", include_str!("fixtures/lint/small_clean.rs")),
    ("small/dirty", include_str!("fixtures/lint/small_dirty.rs")),
    (
        "medium/clean",
        include_str!("fixtures/lint/medium_clean.rs"),
    ),
    (
        "medium/dirty",
        include_str!("fixtures/lint/medium_dirty.rs"),
    ),
    ("large/clean", include_str!("fixtures/lint/large_clean.rs")),
    ("large/dirty", include_str!("fixtures/lint/large_dirty.rs")),
];
/// Reorder benchmark fixtures: `(name, source)` pairs, named by reorder state.
///
/// `stable` fixtures are already in canonical order (reorder is a no-op);
/// `dirty` fixtures move many items. The `medium/stable` and `large/stable`
/// fixtures are the reorder output of their clean counterparts, so they are
/// genuine, already-canonical Rust source.
#[allow(dead_code)] // each bench compiles `common` separately, using one set
pub const REORDER_FIXTURES: &[(&str, &str)] = &[
    (
        "small/stable",
        include_str!("fixtures/reorder/small_stable.rs"),
    ),
    (
        "small/dirty",
        include_str!("fixtures/reorder/small_dirty.rs"),
    ),
    (
        "medium/stable",
        include_str!("fixtures/reorder/medium_stable.rs"),
    ),
    (
        "medium/dirty",
        include_str!("fixtures/reorder/medium_dirty.rs"),
    ),
    (
        "large/stable",
        include_str!("fixtures/reorder/large_stable.rs"),
    ),
    (
        "large/dirty",
        include_str!("fixtures/reorder/large_dirty.rs"),
    ),
];

/// Build the crate-aware context (module tree + crate-wide re-export set) for
/// one embedded crate fixture. Parses each embedded source once. Called in the
/// bench's setup (outside `iter`), so the hot loop measures only
/// `narrow_vis_in_tree`.
///
/// # Arguments
///
/// - `sources`: embedded `(path, source)` pairs forming the crate fixture.
///
/// # Returns
///
/// `(tree, reexports, owned)` where `owned` is the owned
/// `(path, source)` pairs the hot loop borrows; `narrow_vis_in_tree` returns
/// narrowed output separately.
#[allow(dead_code)]
pub fn build_crate_context(
    sources: &[(&str, &str)],
) -> (ModuleTree, ReexportSet, Vec<(String, String)>) {
    let owned: Vec<(String, String)> = sources
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect();
    // Parse each file once into a `ParsedFile` shared by the module-tree build
    // and the crate-wide re-export scan (single parse per file).
    let files: Vec<ParsedFile> = owned
        .iter()
        .map(|(p, s)| {
            ParsedFile::new(std::path::PathBuf::from(p), s.clone()).expect("fixture must parse")
        })
        .collect();
    let root = std::path::PathBuf::from(&owned[0].0);
    let tree = build_module_tree(&root, &files).expect("crate fixture must resolve");
    let reexports = collect_crate_reexports(&files);
    (tree, reexports, owned)
}
