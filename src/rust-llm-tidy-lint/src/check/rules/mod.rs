//! The lint rules: one folder per rule family.
//!
//! - [`text`] - TEXT001/TEXT002 over a measured document.
//! - [`rust`] - DOC001-DOC006 and TEST001 over parsed Rust items.

pub(crate) mod rust;
pub(crate) mod text;
