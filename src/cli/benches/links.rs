//! Benchmarks for the `fix` link-hoist pass.
//!
//! Measures [`fix_links`] over each fixture, mirroring the CLI's `fix_file`
//! link step (run after fence fixing) minus file I/O. `clean` fixtures are
//! borrowed back unchanged; `dirty` fixtures trigger inline-link rewriting plus
//! appended `[text]: url` definitions.
//!
//! [`fix_links`]: rust_llm_tidy_fix::fix_links

criterion_group!(benches, link_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
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
                black_box(out);
            });
        });
    }
    group.finish();
}
