//! Order source items using language-specific policies and source-preserving
//! emission.

pub use emit::{Permutation, compute_moves, emit};
pub use reorder_move::ReorderMove;

pub(crate) mod csharp;
mod emit;
pub mod graph;
mod reorder_move;
pub(crate) mod rust;
