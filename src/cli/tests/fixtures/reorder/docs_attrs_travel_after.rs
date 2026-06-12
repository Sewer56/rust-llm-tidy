//! Rule: Doc comments and attributes travel with the item they annotate.
//!
//! Items before reorder:
//! - fn b() {}
//! - /// Doc for a.
//! - fn a() { b(); }
//!
//! Items after reorder:
//! - /// Doc for a.
//! - fn a() { b(); }
//! - fn b() {}
//!
//! Notes:
//! - The doc comment remains attached to fn a even when fn a moves earlier.
//!
/// Doc for a.
fn a() {
    b();
}

fn b() {}
