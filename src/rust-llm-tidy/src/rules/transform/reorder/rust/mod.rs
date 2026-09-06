//! Rust ordering policy and permutation construction.

use super::{Permutation, graph};
use crate::source::ParseResult;

mod profile;

/// Construct a complete permutation using the Rust phase policy.
pub(crate) fn reorder_permutation(parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
    let order = graph::compute_order(parsed, &profile::RustProfile)?;
    let permutation = Permutation::new(parsed.items.len(), order)?;
    Ok(Some(permutation))
}
