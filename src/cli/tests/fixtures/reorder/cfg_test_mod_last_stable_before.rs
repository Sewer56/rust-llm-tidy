//! Rule: `#[cfg(test)]` modules are kept last and in their original order.
//!
//! Items before reorder:
//! - #[cfg(test)] mod tests_b;
//! - #[cfg(test)] mod tests_a;
//! - fn main() {}
//!
//! Items after reorder:
//! - fn main() {}
//! - #[cfg(test)] mod tests_b;
//! - #[cfg(test)] mod tests_a;
//!
//! Notes:
//! - Test modules live in phase 10, after all other items.
//! - Ordering among test modules is stable.
//!
fn main() {}

#[cfg(test)]
mod tests_b;
#[cfg(test)]
mod tests_a;
