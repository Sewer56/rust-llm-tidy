//! Rule: DOC008 - `# Errors` variants listed out of alphabetical order.
//!
//! A `pub fn` returning `Result` whose `# Errors` section lists the
//! returned in-file enum's variants out of alphabetical order is flagged
//! as an error.
//!
//! Expected diagnostics:
//! - DOC008 on `pub fn load` (lists [Error::NotFound] before [Error::Denied])
//!
//! Not flagged (should pass):
//! - `pub fn save` (variants listed alphabetically)

/// Loads a file.
///
/// # Errors
///
/// Returns [Error::NotFound] if the file does not exist and
/// [Error::Denied] when access is refused.
pub fn load() -> Result<(), Error> {
    Ok(())
}

/// Saves a file.
///
/// # Errors
///
/// Returns [Error::Denied] when access is refused and
/// [Error::NotFound] if the file does not exist.
pub fn save() -> Result<(), Error> {
    Ok(())
}

/// An error type.
pub enum Error {
    /// Access refused.
    Denied,
    /// Not found.
    NotFound,
}
