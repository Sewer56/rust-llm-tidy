//! Rule: Type-like items are dependency-sorted within their phase; the referencer precedes the referenced item.
//!
//! Items before reorder:
//! - struct A;
//! - struct B { a: A }
//!
//! Items after reorder:
//! - struct B { a: A }
//! - struct A;
//!
//! Notes:
//! - Struct B references struct A; the referencer (B) is emitted first, matching caller-before-callee ordering.
//!
struct B {
    a: A,
}

struct A;
