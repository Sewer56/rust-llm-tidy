//! Rule: `main` is placed first inside its visibility group regardless of call graph.
//!
//! Items before reorder:
//! - pub fn helper() { main(); }
//! - pub fn main() {}
//!
//! Items after reorder:
//! - pub fn main() {}
//! - pub fn helper() { main(); }
//!
//! Notes:
//! - `main` is treated as an entry point and sorted before all other functions in the same visibility group.
//! - helper calls main, but `main` still comes first.
//!
pub fn main() {}

pub fn helper() {
    main();
}
