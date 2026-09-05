//! Rule: `use`, `mod`, and `const`/`static`/`extern` are each compact internally; a blank line separates different groups.
//!
//! Items before reorder:
//! - use std::fmt;
//! - use crate::foo::Bar;
//! - mod alpha;
//! - mod beta;
//! - const B: i32 = 0;
//! - const A: i32 = 0;
//! - static C: i32 = 0;
//!
//! Items after reorder:
//! - use crate::foo::Bar;
//! - use std::fmt;
//! - mod alpha;
//! - mod beta;
//! - const A: i32 = 0;
//! - const B: i32 = 0;
//! - static C: i32 = 0;
//!
//! Notes:
//! - `use` items are compact with each other.
//! - `mod` items are compact with each other.
//! - `const`/`static` items are compact with each other.
//! - A blank line separates `use` from `mod` and `mod` from `const`/`static`.
//!
use crate::foo::Bar;
use std::fmt;

mod alpha;
mod beta;

const A: i32 = 0;
const B: i32 = 0;
static C: i32 = 0;
