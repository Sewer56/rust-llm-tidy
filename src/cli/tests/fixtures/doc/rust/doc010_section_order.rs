//! Rule: DOC010 - doc sections listed out of canonical order.
//!
//! A public item whose doc comments list recognized section headers out
//! of canonical order is flagged as an error.
//!
//! Expected diagnostics:
//! - DOC010 on `pub fn load` (`# Errors` before `# Arguments`)
//!
//! Not flagged (should pass):
//! - `pub fn save` (sections in canonical order)

/// Saves the buffered lines to the target file.
///
/// # Arguments
///
/// - `path` - destination file path, created or truncated.
///
/// # Errors
///
/// Returns [Error::Io] when the file cannot be written.
pub fn save(path: &str) -> Result<(), Error> {
    Ok(())
}

/// Loads the file's lines into a buffer.
///
/// # Errors
///
/// Returns [Error::Io] when the file cannot be read.
///
/// # Arguments
///
/// - `path` - source file path to read.
pub fn load(path: &str) -> Result<(), Error> {
    Ok(())
}

/// An error type.
pub enum Error {
    /// An I/O failure.
    Io,
}
