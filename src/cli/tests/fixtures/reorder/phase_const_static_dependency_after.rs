//! Rule: `const` and `static` items are dependency-sorted within their phase; the referencer precedes the referenced item.
//!
//! Items before reorder:
//! - const A: i32 = 0;
//! - const B: i32 = A;
//!
//! Items after reorder:
//! - const B: i32 = A;
//! - const A: i32 = 0;
//!
//! Notes:
//! - const B references const A; the referencer (B) is emitted first, matching caller-before-callee ordering.
//!
const B: i32 = A;
const A: i32 = 0;
