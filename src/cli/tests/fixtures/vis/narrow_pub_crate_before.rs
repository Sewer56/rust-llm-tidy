//! Fixture: bare `pub` inside `pub(crate)` mod narrows to `pub(crate)`.
pub(crate) mod m {
    pub fn f() {}

    pub struct S;

    pub const C: u32 = 0;
}
