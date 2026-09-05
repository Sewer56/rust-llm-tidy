//! Benchmarks for the `check` CLI operation.
//!
//! Measures the full lint pass over each fixture: parse the source with
//! the Rust backend, then run its lint composition (the item rules plus
//! the Ast text tier). This mirrors the CLI's `check_file` dispatch minus
//! file I/O.

criterion_group!(benches, lint_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy_lang::backends::LanguageBackend;
use rust_llm_tidy_lang::backends::rust::RustBackend;

#[path = "common.rs"]
mod common;

/// Benchmark the lint pass (parse + `RustBackend::lint`) per fixture.
fn lint_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("lint");
    for (name, source) in common::LINT_FIXTURES {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let parsed = RustBackend.parse(source).expect("fixture must parse");
                let diagnostics = RustBackend.lint(&parsed);
                std::hint::black_box(diagnostics);
            });
        });
    }
    group.finish();
}
