//! Rule: Trailing content after the last item stays at the bottom of the file.
//!
//! Items before reorder:
//! - fn b() { a(); }
//! - fn a() {}
//! - // trailing comment
//!
//! Items after reorder:
//! - fn b() { a(); }
//! - fn a() {}
//! - // trailing comment
//!
//! Notes:
//! - Comments after the last item are preserved in the trailer.
//!
fn b() {
    a();
}

fn a() {}

// trailing comment
