//! Fixture: an item re-exported via `pub use` at the crate root is NOT narrowed.
pub use inner::f;

pub(crate) mod inner {
    pub fn f() {}
}
