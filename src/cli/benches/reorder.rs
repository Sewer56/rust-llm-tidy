//! Benchmarks for the `reorder` CLI operation.
//!
//! Measures the full reorder pass over each fixture:
//!
//! - parse the source
//! - compute the item permutation through the backend's ordering policy
//!   ([`LanguageBackend::reorder_permutation`])
//! - [`emit`] the reordered source
//! - run the line-preservation [`verify_line_preservation`] safety check
//!
//! This mirrors the CLI's `reorder_file` path minus file I/O.
//!
//! [`emit`]: rust_llm_tidy_reorder::reorder::emit
//! [`verify_line_preservation`]: rust_llm_tidy_model::safety::verify_line_preservation

criterion_group!(benches, reorder_pass);

criterion_main!(benches);

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy_lang::{LanguageBackend, RustBackend};
use rust_llm_tidy_model::safety;
use rust_llm_tidy_reorder::reorder::emit;

#[path = "common.rs"]
mod common;

/// Benchmark the reorder pass (`RustBackend.parse` +
/// [`LanguageBackend::reorder_permutation`] + [`emit`] +
/// [`verify_line_preservation`]) per fixture.
fn reorder_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("reorder");
    for (name, source) in common::REORDER_FIXTURES {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_function(*name, |bencher| {
            bencher.iter(|| {
                let parsed = RustBackend.parse(source).expect("fixture must parse");
                let permutation = RustBackend
                    .reorder_permutation(&parsed)
                    .expect("order must compute")
                    .expect("the Rust backend always reorders");
                let output = emit(&parsed, &permutation).expect("emit must succeed");
                safety::verify_line_preservation(source, &output).expect("lines must be preserved");
                std::hint::black_box(output);
            });
        });
    }
    group.finish();
}
