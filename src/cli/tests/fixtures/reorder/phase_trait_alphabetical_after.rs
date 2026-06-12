//! Rule: Independent traits are sorted alphabetically by trait name.
//!
//! Items before reorder:
//! - trait C {}
//! - trait A {}
//! - trait B {}
//!
//! Items after reorder:
//! - trait A {}
//! - trait B {}
//! - trait C {}
//!
//! Notes:
//! - Phase 7 applies alphabetical tie-breaking when there are no dependencies.
//!
trait A {}

trait B {}

trait C {}
