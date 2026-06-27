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
//!
//! Each fixture is a real `.rs` file from an open-source project, embedded
//! verbatim with [`include_str!`] (byte-exact copies, so the benchmarks
//! reflect realistic parse characteristics). Provenance (repo, path, pinned
//! permalink) is documented in the header comment of each fixture file.

use rust_llm_tidy_vis::{ModuleTree, ReexportSet};

/// Multi-file crate fixtures for the crate-aware vis bench: each entry is a
/// small crate as `(name, root_relative_path, [(path, source)])`. The parent
/// declares `pub(crate) mod foo;`; `foo.rs` holds bare-`pub` children that the
/// crate-aware pass narrows cross-file.
///
/// Sources are embedded inline (no filesystem I/O at bench time): the resolver
/// API `build_module_tree(root, &[(path, source)])` accepts in-memory pairs.
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
/// Returns `(tree, reexports, owned)` where `owned` is the owned
/// `(path, source)` pairs narrowed per file in the hot loop.
#[allow(dead_code)]
pub fn build_crate_context(
    sources: &[(&str, &str)],
) -> (ModuleTree, ReexportSet, Vec<(String, String)>) {
    use rust_llm_tidy_vis::{build_module_tree, collect_crate_reexports};
    let owned: Vec<(String, String)> = sources
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect();
    let parsed: Vec<syn::File> = owned
        .iter()
        .filter_map(|(_, s)| syn::parse_str(s).ok())
        .collect();
    let root = std::path::PathBuf::from(&owned[0].0);
    let tree = build_module_tree(
        &root,
        &owned
            .iter()
            .map(|(p, s)| (std::path::PathBuf::from(p), s.clone()))
            .collect::<Vec<_>>(),
    )
    .expect("crate fixture must resolve");
    let reexports = collect_crate_reexports(parsed.iter());
    (tree, reexports, owned)
}

/// Force the [`proc_macro2`] fallback span impl once.
///
/// Mirrors the CLI's [`main`], which calls [`proc_macro2::fallback::force`] so
/// that byte-range spans are accurate when parsing outside a proc-macro context.
///
/// [`main`]: rust_llm_tidy_cli
#[allow(dead_code)] // each bench compiles `common` separately; some may not call this
pub fn force_span_fallback() {
    proc_macro2::fallback::force();
}
