//! Fixture: bare `pub` inside `pub(crate)` mod narrows to `pub(crate)`.
pub(crate) mod m {
    pub(crate) fn f() {}

    pub(crate) struct S;

    pub(crate) const C: u32 = 0;
}
