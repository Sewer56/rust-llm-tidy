//! Rule: Inline `#[cfg(test)]` module definitions are moved to the end of
//! the file, keeping their original relative order.
//!
//! Items before reorder:
//! - #[cfg(test)] mod tests_b { ... }
//! - #[cfg(test)] mod tests_a { ... }
//! - fn main() {}
//!
//! Items after reorder:
//! - fn main() {}
//! - #[cfg(test)] mod tests_b { ... }
//! - #[cfg(test)] mod tests_a { ... }
//!
//! Notes:
//! - Inline test-module definitions live in the final phase, after all other
//!   items.
//! - Ordering among the test modules is stable.
//!

#[cfg(test)]
mod tests_b {
    fn helper() {}
}

#[cfg(test)]
mod tests_a {
    fn helper() {}
}

fn main() {}
