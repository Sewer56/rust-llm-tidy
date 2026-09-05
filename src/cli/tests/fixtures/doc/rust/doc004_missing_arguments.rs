//! Rule: DOC004 - pub fn with parameters must have a `# Arguments` section.
//!
//! A public function that declares at least one named parameter should
//! document those parameters under an `# Arguments` (or `# Parameters`)
//! heading. Public functions with no parameters are not flagged.
//!
//! Expected diagnostics:
//! - DOC004 on `pub fn greet` (has param `name`, no # Arguments)
//!
//! Not flagged (should pass):
//! - `pub fn no_args` (no parameters, no # Arguments needed)

/// Greets a user by name.
pub fn greet(name: &str) -> String {
    format!("hi {name}")
}

/// Returns a fixed greeting.
pub fn no_args() -> String {
    String::from("hi")
}
