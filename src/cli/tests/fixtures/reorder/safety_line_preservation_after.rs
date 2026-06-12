//! Rule: Reordering preserves every non-blank line from the original file.
//!
//! Items before reorder:
//! - fn a() {}
//! - fn b() { // call helper; a(); }
//!
//! Items after reorder:
//! - fn b() { // call helper; a(); }
//! - fn a() {}
//!
//! Non-blank lines preserved by the safety pass:
//! - `fn a() {}`
//! - `fn b() {`
//! - `// call helper`
//! - `a();`
//! - `}`
//!
//! Notes:
//! - The safety pass verifies the multiset of non-blank lines (including body comments) is unchanged.
//!
fn b() {
    // call helper
    a();
}

fn a() {}
