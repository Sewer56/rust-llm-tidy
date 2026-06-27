//! Benchmark fixture: bar.rs child for medium crate-aware vis.
//! Bare-`pub` children that get narrowed to pub(crate).
//! Embedded verbatim via include_str! in benches/common.rs.
pub fn g() {}
pub struct Bar;
