//! Shared benchmark fixtures and setup.
//!
//! Fixtures live in per-operation filesystem folders so each benchmark
//! iterates only the files relevant to its operation:
//!
//! - `fixtures/lint/`: named `<size>_<clean|dirty>` by lint outcome.
//! - `fixtures/reorder/`: named `<size>_<stable|dirty>` by reorder outcome
//!   (`stable` = already in canonical order, so reorder is a no-op).
//!
//! Each fixture is a real `.rs` file from an open-source project, embedded
//! verbatim with [`include_str!`] (byte-exact copies, so the benchmarks
//! reflect realistic parse characteristics). Provenance (repo, path, pinned
//! permalink) is documented in the header comment of each fixture file.

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

/// Force the [`proc_macro2`] fallback span impl once.
///
/// Mirrors the CLI's [`main`], which calls [`proc_macro2::fallback::force`] so
/// that byte-range spans are accurate when parsing outside a proc-macro context.
///
/// [`main`]: rust_llm_tidy_cli
#[allow(dead_code)] // each bench compiles `common` separately; the fences bench does not call this
pub fn force_span_fallback() {
    proc_macro2::fallback::force();
}
