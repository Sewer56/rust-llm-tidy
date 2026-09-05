//! Rule: Unsupported top-level items keep their original file order.
//!
//! Items before reorder:
//! - extern "C" {}
//! - extern "Rust" {}
//!
//! Items after reorder:
//! - extern "C" {}
//! - extern "Rust" {}
//!
//! Notes:
//! - Foreign blocks map to the `Other` kind and use stable ordering.
//!
extern "C" {}

extern "Rust" {}
