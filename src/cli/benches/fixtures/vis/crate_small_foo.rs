//! Benchmark fixture: foo.rs child for small crate-aware vis.
//! Bare-`pub` child that gets narrowed to pub(crate) by the tree floor.
//! Embedded verbatim via include_str! in benches/common.rs.
pub fn f() {}
pub struct S;
