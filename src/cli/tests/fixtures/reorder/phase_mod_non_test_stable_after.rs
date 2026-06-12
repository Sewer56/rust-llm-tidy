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
//! - Phase 3 covers `mod` items that are not gated by `#[cfg(test)]`.
//! - Order is preserved as-is; alphabetical sorting is delegated to rustfmt.
//!
mod beta;
mod alpha;
