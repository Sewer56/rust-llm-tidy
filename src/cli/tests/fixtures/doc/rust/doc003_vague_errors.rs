//! Rule: DOC003 - `# Errors` sections should name concrete error variants.
//!
//! A `# Errors` section that exists but does not reference any concrete
//! variant (via a rustdoc link `[...]` or a path `::`) is vague and should
//! be flagged as a warning. Sections that name variants pass.
//!
//! Expected diagnostics:
//! - DOC003 on `pub fn vague_load` (# Errors present, no variant named)
//!
//! Not flagged (should pass):
//! - `pub fn specific_link` (# Errors names [Error::NotFound])
//! - `pub fn specific_path` (# Errors names Error::Timeout via ::)
//! - `pub fn plain` (non-Result, not checked)

/// Loads a file.
///
/// # Errors
///
/// Returns an error if loading fails.
pub fn vague_load() -> Result<(), Error> {
    Ok(())
}

/// Loads a file.
///
/// # Errors
///
/// Returns [Error::NotFound] if the file does not exist.
pub fn specific_link() -> Result<(), Error> {
    Ok(())
}

/// Loads a file.
///
/// # Errors
///
/// Returns `Error::Timeout` if the server does not respond.
pub fn specific_path() -> Result<(), Error> {
    Ok(())
}

/// Does something plain.
pub fn plain() {}

/// An error type.
pub enum Error {
    /// Not found.
    NotFound,
    /// Timed out.
    Timeout,
}
