//! Rule: For the same target type, inherent `impl` blocks come before trait `impl` blocks.
//!
//! Items before reorder:
//! - struct T;
//! - trait U {}
//! - impl U for T {}
//! - impl T {}
//!
//! Items after reorder:
//! - struct T;
//! - trait U {}
//! - impl T {}
//! - impl U for T {}
//!
//! Notes:
//! - Inherent impls are placed before trait impls after the matching type.
//!
struct T;

trait U {}

impl T {}

impl U for T {}
