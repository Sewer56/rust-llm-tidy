//! Benchmark fixture: medium crate-aware lib.rs for cross-file vis.
//! Declares `pub(crate) mod foo;` and `pub(crate) mod bar;`.
//! Both child files inherit the same floor.
//! Embedded verbatim via include_str! in benches/common.rs.
pub(crate) mod foo;
pub(crate) mod bar;
