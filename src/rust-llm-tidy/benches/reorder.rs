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
//! [`emit`]: rust_llm_tidy::rules::transform::reorder::emit
//! [`verify_line_preservation`]: rust_llm_tidy::source::preservation::verify_line_preservation

criterion_group!(benches, reorder_pass);

criterion_main!(benches);

use core::hint;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rust_llm_tidy::languages::{LanguageBackend, RustBackend};
use rust_llm_tidy::rules::transform::reorder::emit;
use rust_llm_tidy::source::preservation as safety;

#[path = "fixture_setup/languages.rs"]
mod fixtures;

/// Benchmark the reorder pass (`RustBackend.parse` +
/// [`LanguageBackend::reorder_permutation`] + [`emit`] +
/// [`verify_line_preservation`]) per fixture.
fn reorder_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("reorder");
    for (name, source) in fixtures::REORDER_FIXTURES {
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
                hint::black_box(output);
            });
        });
    }
    group.finish();
}
