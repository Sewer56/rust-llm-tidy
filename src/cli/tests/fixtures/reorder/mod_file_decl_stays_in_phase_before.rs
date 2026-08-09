//! Rule: File-based `mod` declarations - including `#[cfg(test)]` ones - stay
//! in the mod phase; rustfmt owns their alphabetical order.
//!
//! Items before reorder:
//! - mod beta;
//! - #[cfg(test)] mod test_helpers;
//! - mod zeta;
//!
//! Items after reorder:
//! - mod beta;
//! - #[cfg(test)] mod test_helpers;
//! - mod zeta;   (unchanged - no move)
//!
//! Notes:
//! - File-based mod declarations (test or not) never move to the end, even
//!   when `#[cfg(test)]` is present.
//! - Among mod declarations, rustfmt keeps alphabetical order.
//!

mod beta;
#[cfg(test)]
mod test_helpers;
mod zeta;
