//! Benchmarks for the `fix` fence pass.
//!
//! Measures [`fix_fences`] over each fixture, mirroring the CLI's `fix_file`
//! fence step (run after table alignment) minus file I/O. `clean` fixtures are
//! borrowed back unchanged; `dirty` fixtures trigger marker rewriting.
//!
//! [`fix_fences`]: rust_llm_tidy_fix::fix_fences

criterion_group!(benches, fence_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy_fix::fix_fences;

#[path = "common.rs"]
mod common;

/// Benchmark [`fix_fences`] per fixture.
fn fence_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("fences");
    for (name, source) in common::FENCE_FIXTURES {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let out = fix_fences(source);
                std::hint::black_box(out);
            });
        });
    }
    group.finish();
}
