#![doc = include_str!(concat!("../", env!("CARGO_PKG_README")))]

pub use reorder::Permutation;
/// Re-exported shared source model (parse, io, safety).
///
/// These modules live in the dedicated [`rust_source_model`] crate. They are
/// re-exported here so that the original public API paths
/// (`rust_auto_reorder::parse`, `rust_auto_reorder::io`,
/// `rust_auto_reorder::safety`) keep resolving for existing consumers, and so
/// internal modules can refer to them as `crate::parse` etc.
pub use rust_source_model::{io, parse, safety};

pub mod graph;
pub mod reorder;
