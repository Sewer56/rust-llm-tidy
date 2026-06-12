//! Rule: Callers are placed before callees within the same visibility group.
//!
//! Items before reorder:
//! - fn a() {}
//! - fn b() { a(); }
//!
//! Items after reorder:
//! - fn b() { a(); }
//! - fn a() {}
//!
//! Notes:
//! - Function b calls function a, so b is emitted first after reorder.
//!
fn b() {
    a();
}

fn a() {}
