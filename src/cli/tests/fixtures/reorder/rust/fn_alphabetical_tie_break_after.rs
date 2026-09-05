//! Rule: Functions with no dependencies are sorted alphabetically within their visibility group.
//!
//! Items before reorder:
//! - fn c() {}
//! - fn a() {}
//! - fn b() {}
//!
//! Items after reorder:
//! - fn a() {}
//! - fn b() {}
//! - fn c() {}
//!
//! Notes:
//! - Alphabetical tie-breaking is used for zero-in-degree nodes.
//!
fn a() {}

fn b() {}

fn c() {}
