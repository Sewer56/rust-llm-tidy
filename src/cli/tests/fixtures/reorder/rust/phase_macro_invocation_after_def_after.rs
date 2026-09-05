//! Rule: A top-level macro invocation follows its `macro_rules!` definition.
//!
//! Items before reorder:
//! - use std::fs;
//! - static COUNT ...
//! - macro_rules! synthetic_fixture { ... }
//! - synthetic_fixture!(alpha);
//!
//! Items after reorder:
//! - use std::fs;
//! - macro_rules! synthetic_fixture { ... }
//! - synthetic_fixture!(alpha);
//! - static COUNT ...
//!
//! Notes:
//! - Rust `macro_rules!` use textual scoping: a definition must precede its
//!   invocation. Phase 4 places the local invocation right after its
//!   definition. External macros (no local def) stay in phase 1.
//!
use std::fs;

macro_rules! synthetic_fixture {
    ($name:ident) => {};
}

synthetic_fixture!(alpha);

static COUNT: i32 = 0;
