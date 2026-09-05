//! Rule: `use` statements keep their original source order (not reordered).
//!
//! The tool does not sort imports - rustfmt controls use ordering. `use`
//! items are emitted in a contiguous block but keep their original relative
//! order.
//!
//! Items before reorder:
//! - use std::fmt;
//! - use crate::foo::Bar;
//! - use ahash::AHashMap;
//!
//! Items after reorder:
//! - use std::fmt;
//! - use crate::foo::Bar;
//! - use ahash::AHashMap;
//!
//! Notes:
//! - `use` ordering is left to rustfmt; this tool preserves source order.
//!
use crate::foo::Bar;
use ahash::AHashMap;
use std::fmt;
