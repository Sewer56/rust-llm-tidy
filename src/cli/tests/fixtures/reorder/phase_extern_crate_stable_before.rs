//! Rule: `extern crate` items keep their original file order.
//!
//! Items before reorder:
//! - extern crate beta;
//! - extern crate alpha;
//!
//! Items after reorder:
//! - extern crate beta;
//! - extern crate alpha;
//!
//! Notes:
//! - Phase 1 uses stable ordering.
//!
extern crate beta;
extern crate alpha;
