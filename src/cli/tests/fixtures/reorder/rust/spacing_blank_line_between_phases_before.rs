//! Rule: A blank line is inserted between items from different phases.
//!
//! Items before reorder:
//! - fn main() {}
//! - struct S;
//! - use std::fmt;
//!
//! Items after reorder:
//! - use std::fmt;
//! - struct S;
//! - fn main() {}
//!
//! Notes:
//! - The input has no blank lines between items; the tool inserts a blank line where phases change.
//!
use std::fmt;

struct S;

fn main() {}
