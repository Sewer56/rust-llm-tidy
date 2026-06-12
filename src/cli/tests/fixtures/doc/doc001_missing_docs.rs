//! Rule: DOC001 - non-private items must have `///` doc comments.
//!
//! Every `pub` item of a documentable kind (fn, struct, enum, union, type,
//! trait, const, static) without a `///` comment should be flagged. Private
//! items must be skipped.
//!
//! Expected diagnostics:
//! - DOC001 on `pub fn alpha` (undocumented fn)
//! - DOC001 on `pub struct Beta` (undocumented struct)
//! - DOC001 on `pub enum Gamma` (undocumented enum)
//! - DOC001 on `pub const DELTA` (undocumented const)
//! - DOC001 on `pub static EPSILON` (undocumented static)
//! - DOC001 on `pub trait Zeta` (undocumented trait)
//! - DOC001 on `pub type Eta` (undocumented type)
//! - DOC001 on `pub union Theta` (undocumented union)
//!
//! Not flagged (should pass):
//! - `fn helper` (private)
//! - `pub fn documented` (has `///`)
//! - `pub use std::collections::HashMap` (use is not documentable)

pub fn alpha() {}

fn helper() {}

/// Documented.
pub fn documented() {}

pub struct Beta {
    x: i32,
}

pub enum Gamma {
    A,
    B,
}

pub const DELTA: u32 = 0;

pub static EPSILON: u32 = 0;

pub trait Zeta {
    fn method(&self);
}

pub type Eta = i32;

pub union Theta {
    a: u32,
    b: [u8; 4],
}

pub use std::collections::HashMap;
