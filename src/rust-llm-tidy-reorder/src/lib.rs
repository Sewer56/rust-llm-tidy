#![doc = include_str!(concat!("../", env!("CARGO_PKG_README")))]

pub use reorder::Permutation;

pub mod graph;
pub mod reorder;
