//! Order source items using language-specific policies and source-preserving
//! emission.

pub use emit::{Permutation, compute_moves, emit};
pub use reorder_move::ReorderMove;

mod emit;
pub mod graph;
mod reorder_move;
