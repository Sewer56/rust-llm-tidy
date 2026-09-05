//! Rule: `struct`, `enum`, `union`, and `type` are sorted alphabetically when independent.
//!
//! Items before reorder:
//! - struct C;
//! - enum B {}
//! - union A { f: u32 }
//! - type D = i32;
//!
//! Items after reorder:
//! - union A { ... }
//! - enum B {}
//! - struct C;
//! - type D = i32;
//!
//! Notes:
//! - Phase 6 groups all type-like items together.
//!
union A { f: u32 }

enum B {}

struct C;

type D = i32;
