//! Benchmark fixture: small crate-aware lib.rs for cross-file vis.
//! Declares `pub(crate) mod foo;` so the child file inherits the floor.
//! Embedded verbatim via include_str! in benches/common.rs.
pub(crate) mod foo;
