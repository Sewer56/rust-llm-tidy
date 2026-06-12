//! Rule: Functions are grouped by visibility: pub, then pub(restricted), then private.
//!
//! Items before reorder:
//! - fn private_fn() {}
//! - pub fn public_fn() {}
//! - pub(crate) fn restricted_fn() {}
//!
//! Items after reorder:
//! - pub fn public_fn() {}
//! - pub(crate) fn restricted_fn() {}
//! - fn private_fn() {}
//!
//! Notes:
//! - Each visibility tier is a separate sub-group ordered by dependencies and tie-breaks.
//!
pub fn public_fn() {}

pub(crate) fn restricted_fn() {}

fn private_fn() {}
