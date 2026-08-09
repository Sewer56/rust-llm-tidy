//! Rule: Non-test `mod` declarations keep their original (stable) order;
//! sorting is left to rustfmt.
//!
//! Items before reorder:
//! - mod beta;
//! - mod alpha;
//!
//! Items after reorder:
//! - mod beta;
//! - mod alpha;   (unchanged - stable)
//!
//! Notes:
//! - Phase 3 covers file-based `mod` declarations, whether `#[cfg(test)]` is
//!   present or not, plus inline non-test mods.
//! - File-based declarations stay in their original position; alphabetical
//!   sorting is delegated to rustfmt.
//!
mod beta;
mod alpha;
