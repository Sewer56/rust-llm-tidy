//! Benchmarks for the `check` CLI operation.
//!
//! Measures the full lint pass over each fixture: parse the source with
//! [`parse_source`], then run every documentation check with
//! [`run_all`]. This mirrors the CLI's `check_file` path minus file I/O.
//!
//! [`parse_source`]: rust_llm_tidy_model::parse::parse_source
//! [`run_all`]: rust_llm_tidy_lint::check::run_all

criterion_group!(benches, lint_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use rust_llm_tidy_lint::check;
use rust_llm_tidy_model::parse;

#[path = "common.rs"]
mod common;

/// Benchmark the lint pass (`parse_source` + [`check::run_all`]) per fixture.
fn lint_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("lint");
    for (name, source) in common::LINT_FIXTURES {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let parsed = parse::parse_source(source).expect("fixture must parse");
                let diagnostics = check::run_all(&parsed);
                black_box(diagnostics);
            });
        });
    }
    group.finish();
}
