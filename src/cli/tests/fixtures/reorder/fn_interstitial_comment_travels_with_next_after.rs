//! Rule: A plain `//` comment between two functions travels with the NEXT item it precedes when items are reordered.
//!
//! Items before reorder:
//! - fn b_helper() {}
//! - // === HELPERS ===
//! - fn a_main() { b_helper(); }
//!
//! Items after reorder:
//! - // === HELPERS ===
//! - fn a_main() { b_helper(); }
//! - fn b_helper() {}
//!
//! Notes:
//! - The comment is a header for the next item (a_main); after reorder it stays directly above a_main.
//!
// === HELPERS ===

fn a_main() { b_helper(); }

fn b_helper() {}
