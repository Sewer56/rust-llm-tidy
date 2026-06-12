//! Rule: Impl blocks for types not defined in the file keep their original order.
//!
//! Items before reorder:
//! - impl X {}
//! - impl Y {}
//!
//! Items after reorder:
//! - impl X {}
//! - impl Y {}
//!
//! Notes:
//! - Orphan impls use stable ordering within their sub-phase.
//!
impl X {}

impl Y {}
