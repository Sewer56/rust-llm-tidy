//! Benchmarks for the `vis` pass (crate-aware path).
//!
//! Measures [`narrow_vis_in_tree`] per file across the multi-file crate
//! fixtures, mirroring the CLI's crate-aware `vis` step minus file I/O and the
//! one-time crate-context build. The crate context (module tree + crate-wide
//! re-export set) is built once per fixture in setup; the hot loop measures only
//! the per-file narrowing.
//!
//! [`narrow_vis_in_tree`]: rust_llm_tidy_vis::narrow_vis_in_tree

criterion_group!(benches, vis_crate_aware);

criterion_main!(benches);

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use rust_llm_tidy_vis::narrow_vis_in_tree;

#[path = "common.rs"]
mod common;

/// Benchmark [`narrow_vis_in_tree`] per file across the multi-file crate
/// fixtures. The crate context (module tree + crate-wide re-export set) is built
/// once per fixture in setup; the hot loop measures only the per-file narrowing.
fn vis_crate_aware(c: &mut Criterion) {
    common::force_span_fallback();
    let mut group = c.benchmark_group("vis_crate_aware");
    for (name, sources) in common::CRATE_FIXTURES {
        let (tree, reexports, owned) = common::build_crate_context(sources);
        let total_bytes: u64 = owned.iter().map(|(_, s)| s.len() as u64).sum();
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                for (path, src) in &owned {
                    let floor = tree.floor_for(std::path::Path::new(path));
                    let out =
                        narrow_vis_in_tree(src, floor, &reexports).expect("fixture must parse");
                    black_box(out);
                }
            });
        });
    }
    group.finish();
}
