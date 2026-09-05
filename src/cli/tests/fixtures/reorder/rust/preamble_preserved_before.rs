//! Rule: Module-level docs and inner attributes in the preamble stay at the top.
//!
//! Items before reorder:
//! - #![deny(unsafe_code)]
//! - fn a() {}
//! - fn b() { a(); }
//!
//! Items after reorder:
//! - #![deny(unsafe_code)]
//! - fn b() { a(); }
//! - fn a() {}
//!
//! Notes:
//! - The `//!` fixture documentation above is also preserved as part of the preamble.
//!
#![deny(unsafe_code)]

fn b() {
    a();
}

fn a() {}
