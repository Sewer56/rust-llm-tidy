//! Rule: `const` and `static` items are sorted alphabetically when independent.
//!
//! Items before reorder:
//! - static D: i32 = 0;
//! - const C: i32 = 0;
//! - static B: i32 = 0;
//! - const A: i32 = 0;
//!
//! Items after reorder:
//! - const A: i32 = 0;
//! - static B: i32 = 0;
//! - const C: i32 = 0;
//! - static D: i32 = 0;
//!
//! Notes:
//! - Phase 5 groups const and static together and sorts alphabetically.
//!
const A: i32 = 0;
static B: i32 = 0;
const C: i32 = 0;
static D: i32 = 0;
