//! Rule: Function visibility tiers are separated by a blank line.
//!
//! Items before reorder:
//! - fn private_fn() {}
//! - pub fn public_fn() {}
//!
//! Items after reorder:
//! - pub fn public_fn() {}
//! - fn private_fn() {}
//!
//! Notes:
//! - The blank line is part of canonical spacing for non-compact kinds.
//!
pub fn public_fn() {}

fn private_fn() {}
