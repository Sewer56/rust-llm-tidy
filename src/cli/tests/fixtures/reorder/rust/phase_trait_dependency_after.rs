//! Rule: Traits are dependency-sorted within their phase; the referencer precedes the referenced item.
//!
//! Items before reorder:
//! - trait A {}
//! - trait B: A {}
//!
//! Items after reorder:
//! - trait B: A {}
//! - trait A {}
//!
//! Notes:
//! - Trait B references trait A as a supertrait; the referencer (B) is emitted first, matching caller-before-callee ordering.
//!
trait B: A {}

trait A {}
