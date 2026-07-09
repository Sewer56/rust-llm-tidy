//! Benchmarks for the `reorder` CLI operation.
//!
//! Measures the full reorder pass over each fixture: parse the source, build the
//! reference graph and topologically sort with [`compute_order`], construct a
//! [`Permutation`], [`emit`] the reordered source, and run the line-preservation
//! [`verify_line_preservation`] safety check. This mirrors the CLI's
//! `reorder_file` path minus file I/O.
//!
//! [`compute_order`]: rust_llm_tidy_reorder::graph::compute_order
//! [`emit`]: rust_llm_tidy_reorder::reorder::emit
//! [`verify_line_preservation`]: rust_llm_tidy_model::safety::verify_line_preservation

criterion_group!(benches, reorder_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy_model::{parse, safety};
use rust_llm_tidy_reorder::graph;
use rust_llm_tidy_reorder::reorder::{Permutation, emit};

#[path = "common.rs"]
mod common;

/// Benchmark the reorder pass (`parse_source` + [`compute_order`] +
/// [`Permutation::new`] + [`emit`] + [`verify_line_preservation`]) per fixture.
fn reorder_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("reorder");
    for (name, source) in common::REORDER_FIXTURES {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let parsed = parse::parse_source(source).expect("fixture must parse");
                let order = graph::compute_order(&parsed).expect("order must compute");
                let permutation =
                    Permutation::new(parsed.items.len(), order).expect("permutation must build");
                let output = emit(&parsed, &permutation).expect("emit must succeed");
                safety::verify_line_preservation(source, &output).expect("lines must be preserved");
                std::hint::black_box(output);
            });
        });
    }
    group.finish();
}
