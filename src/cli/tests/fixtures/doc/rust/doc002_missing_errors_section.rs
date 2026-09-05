//! Rule: DOC002 - `pub fn` returning `Result` must have a `# Errors` section.
//!
//! A public function whose return type ends in `Result` must document the
//! errors it can return under a `# Errors` heading. Private functions
//! returning `Result` and public functions returning non-`Result` types are
//! not flagged.
//!
//! Expected diagnostics:
//! - DOC002 on `pub fn load` (Result, no # Errors)
//! - DOC002 on `pub fn fetch` (fully-qualified std::result::Result, no # Errors)
//!
//! Not flagged (should pass):
//! - `fn load_private` (private, Result)
//! - `pub fn count` (pub, non-Result)
//! - `pub fn save` (pub, Result, has # Errors)

/// Loads a file.
pub fn load() -> Result<(), String> {
    Ok(())
}

/// Fetches data.
pub fn fetch() -> std::result::Result<u32, std::io::Error> {
    Ok(0)
}

/// Loads data (private).
fn load_private() -> Result<(), String> {
    Ok(())
}

/// Counts items.
pub fn count() -> u32 {
    0
}

/// Saves data.
///
/// # Errors
///
/// Returns [Error::WriteFailed] if the write fails.
pub fn save() -> Result<(), Error> {
    Ok(())
}

/// An error type.
pub enum Error {
    /// A write failed.
    WriteFailed,
}
