//! Rule: DOC006 - doc comments must not contain placeholder text.
//!
//! Placeholder markers (`TODO`, `FIXME`, `TBD`) in doc comments ship
//! incomplete documentation that reads as finished. Items whose doc comments
//! contain a placeholder are flagged.
//!
//! Expected diagnostics:
//! - DOC006 on `pub fn todo_task` (TODO in doc)
//! - DOC006 on `pub fn fixme_task` (FIXME in doc)
//! - DOC006 on `pub fn tbd_task` (TBD in doc)
//!
//! Not flagged (should pass):
//! - `pub fn done` (clean doc)
//! - `pub struct Placeholder` (literal `...` is idiomatic, not a marker)

/// TODO: implement this.
pub fn todo_task() {}

/// FIXME: this is broken.
pub fn fixme_task() {}

/// TBD: what should this do?
pub fn tbd_task() {}

/// Something ... to fill in later.
pub struct Placeholder {
    x: i32,
}

/// A complete function with no placeholders.
pub fn done() {}
