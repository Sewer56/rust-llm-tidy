//! Rule: Independent macros are sorted alphabetically by macro name.
//!
//! Items before reorder:
//! - macro_rules! b { ... }
//! - macro_rules! a { ... }
//!
//! Items after reorder:
//! - macro_rules! a { ... }
//! - macro_rules! b { ... }
//!
//! Notes:
//! - Phase 4 applies alphabetical tie-breaking when there are no dependencies.
//!
macro_rules! a {
    () => {};
}

macro_rules! b {
    () => {};
}
