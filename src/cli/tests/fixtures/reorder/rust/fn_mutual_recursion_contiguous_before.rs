//! Rule: Functions in a mutual recursion cycle remain contiguous in the output.
//!
//! Items before reorder:
//! - fn a() { b(); }
//! - fn c() {}
//! - fn b() { a(); }
//!
//! Items after reorder:
//! - fn c() {}
//! - fn a() { b(); }
//! - fn b() { a(); }
//!
//! Notes:
//! - c has no incoming edges and is emitted first.
//! - a and b form a cycle and stay adjacent.
//!
fn c() {}

fn a() {
    b();
}

fn b() {
    a();
}
