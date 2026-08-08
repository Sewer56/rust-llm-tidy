#![doc = include_str!(concat!("../", env!("CARGO_PKG_README")))]

pub use reorder::{Permutation, ReorderMove, compute_moves};

pub mod graph;
pub mod reorder;
