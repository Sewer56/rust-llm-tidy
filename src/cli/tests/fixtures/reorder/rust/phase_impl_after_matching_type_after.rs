//! Rule: `impl` blocks are placed directly after the type they implement.
//!
//! Items before reorder:
//! - impl T {}
//! - struct T;
//!
//! Items after reorder:
//! - struct T;
//! - impl T {}
//!
//! Notes:
//! - Impl blocks follow their matching type definition.
//!
struct T;

impl T {}
