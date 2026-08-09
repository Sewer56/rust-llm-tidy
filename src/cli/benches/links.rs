//! Benchmarks for the `fix` link-hoist pass.
//!
//! Measures [`fix_links`] over each fixture (the CLI's fix-file link step minus
//! file I/O). Links hoist to `[text]` with per-comment definitions in `doc/*`,
//! a trailing block in Markdown; `doc/noop` (reference-style only) is borrowed
//! back unchanged.
//!
//! [`fix_links`]: rust_llm_tidy_fix::fix_links

criterion_group!(benches, link_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy_fix::fix_links;

#[path = "common.rs"]
mod common;

/// Benchmark [`fix_links`] per fixture.
fn link_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("links");
    for (name, source) in common::LINK_FIXTURES {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let out = fix_links(source);
                std::hint::black_box(out);
            });
        });
    }
    group.finish();
}
