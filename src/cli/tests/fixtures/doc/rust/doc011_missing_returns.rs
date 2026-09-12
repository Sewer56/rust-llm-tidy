//! Rule: DOC011 - pub fn returning a documented-value type must have a
//! `# Returns` section.
//!
//! A public function returning a value needs a `# Returns` section
//! explaining what the value represents. Plain `bool` returns only get a
//! reminder; `Self`, `Result<(), _>`, and unit returns are not flagged.
//!
//! Expected diagnostics:
//! - warning[DOC011] on `pub fn double` (returns u32, no # Returns)
//! - reminder[DOC011] on `pub fn is_ready` (returns bool, no # Returns)
//!
//! Not flagged (should pass):
//! - `pub fn label` (has a proper # Returns section)
//! - `pub fn reset` (Result<(), String> with # Errors)
//! - `Builder::builder` (returns Self)
//! - `fn hidden` (private)

/// Doubles the input.
pub fn double(x: u32) -> u32 {
    x * 2
}

/// Reports whether the count is positive.
pub fn is_ready(count: u32) -> bool {
    count > 0
}

/// Labels the id with a fixed prefix.
///
/// # Returns
///
/// The string `item-{id}` for the given id.
pub fn label(id: u32) -> String {
    format!("item-{id}")
}

/// Resets the counter.
///
/// # Errors
///
/// Returns [ResetError::Denied] when the counter is locked.
pub fn reset() -> Result<(), String> {
    Err(String::from("denied"))
}

/// A builder that produces counters.
pub struct Builder;

impl Builder {
    /// Creates a new builder.
    ///
    /// # Returns
    ///
    /// A fresh, empty builder.
    pub fn builder(&self) -> Self {
        Builder
    }
}

fn hidden(x: u32) -> u32 {
    x
}
