//! Order source items using language-specific policies and source-preserving
//! emission.

pub use emit::{Permutation, ReorderMove, compute_moves, emit};

pub(crate) mod csharp;
mod emit;
pub mod graph;
pub(crate) mod rust;
