//! Rule: A non-doc comment between two functions travels with the item it annotates when the items are reordered.
//!
//! Items before reorder:
//! - fn b() {}
//! - // related note
//! - fn a() { b(); }
//!
//! Items after reorder:
//! - fn a() { b(); }
//! - fn b() {}
//! - // related note
//!
//! Notes:
//! - The comment is attached to fn a as trailing trivia; after reorder it becomes part of the trailer.
//!
fn a() {
    b();
}

fn b() {}
// related note
