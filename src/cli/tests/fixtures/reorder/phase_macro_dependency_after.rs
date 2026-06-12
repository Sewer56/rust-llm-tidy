//! Rule: Macro references are reversed so the macro definition precedes its use.
//!
//! Items before reorder:
//! - fn b() { a!(); }
//! - macro_rules! a { ... }
//!
//! Items after reorder:
//! - macro_rules! a { ... }
//! - fn b() { a!(); }
//!
//! Notes:
//! - The dependency edge is inverted to place macros before consumers.
//! - Macros also live in an earlier phase than functions.
//!
macro_rules! a {
    () => {};
}

fn b() {
    a!();
}
