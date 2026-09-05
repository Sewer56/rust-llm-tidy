//! Benchmarks for the `fix_tables` pass.
//!
//! Measures realigning GFM tables over each fixture in two regimes:
//! - `aligned`: the verbatim fixture canonicalised by `fix_tables` (already
//!   aligned, so the idempotent borrowed fast path applies).
//! - `misaligned`: the fixture with table padding collapsed (exercises the
//!   realignment work path).

criterion_group!(benches, fix_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy_fix::fix_tables;

#[path = "common.rs"]
mod common;

/// The Rust doc-comment marker family, matching the fixtures' `///` tables.
const DOC_PREFIXES: &[&str] = &["///", "//!"];

/// Benchmark `fix_tables` over every fixture in both regimes.
fn fix_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("fix");
    for (name, source) in common::MD_FIXTURES.iter().chain(common::RS_FIXTURES.iter()) {
        // The canonical (already-aligned) form: realigning it is a no-op, so
        // `fix_tables` borrows the input back unchanged.
        let canonical = fix_tables(source, DOC_PREFIXES).into_owned();
        // A deliberately misaligned copy: realigning it rebuilds every table.
        let misaligned = common::misalign(source);

        // Setup-only sanity check: misaligning must yield work for `fix_tables`
        // (an Owned result), guarding against a fixture whose tables already
        // survive misaligning unchanged.
        debug_assert!(
            matches!(
                fix_tables(&misaligned, DOC_PREFIXES),
                std::borrow::Cow::Owned(_)
            ),
            "misalign must produce a table fix_tables will realign: {name}"
        );

        group.throughput(Throughput::Bytes(canonical.len() as u64));
        group.bench_function(format!("{name}/aligned"), |bencher| {
            bencher.iter(|| {
                let out = fix_tables(std::hint::black_box(&canonical), DOC_PREFIXES);
                std::hint::black_box(out);
            });
        });

        group.throughput(Throughput::Bytes(misaligned.len() as u64));
        group.bench_function(format!("{name}/misaligned"), |bencher| {
            bencher.iter(|| {
                let out = fix_tables(std::hint::black_box(&misaligned), DOC_PREFIXES);
                std::hint::black_box(out);
            });
        });
    }
    group.finish();
}
