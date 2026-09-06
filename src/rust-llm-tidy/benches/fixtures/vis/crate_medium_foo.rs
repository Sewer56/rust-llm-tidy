//! Benchmark fixture: foo.rs child for medium crate-aware vis.
//! Bare-`pub` children that get narrowed to pub(crate).
//! Embedded verbatim via include_str! in benches/common.rs.
pub fn f() {}
pub struct Foo;
pub const C: u32 = 0;
