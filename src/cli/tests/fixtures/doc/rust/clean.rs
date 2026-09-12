//! Rule: clean file - no diagnostics expected.
//!
//! Every documentable item carries documentation and every `pub fn`
//! returning `Result` documents its errors, so this file yields zero
//! diagnostics. It verifies the checker avoids false positives on
//! well-formed code.

/// A fully documented struct.
pub struct Config {
    /// The port to listen on.
    pub port: u16,
}

/// A fully documented enum.
pub enum Status {
    /// Idle state.
    Idle,
    /// Active state.
    Active,
}

/// Parses a configuration string.
///
/// # Arguments
///
/// `input` - the configuration text to parse.
///
/// # Returns
///
/// A [Config] with the default port when the string is valid.
///
/// # Errors
///
/// Returns [ParseError::Invalid] if the string is not valid.
pub fn parse(input: &str) -> Result<Config, ParseError> {
    Ok(Config { port: 80 })
}

/// A documented trait.
pub trait Handler {
    /// Handles a request.
    fn handle(&self);
}

/// A documented constant.
pub const DEFAULT_PORT: u16 = 80;

/// A documented static.
pub static VERSION: &str = "1.0.0";

/// A documented type alias.
pub type Port = u16;

/// A documented union.
pub union RawBytes {
    /// As a u32.
    as_u32: u32,
    /// As bytes.
    as_bytes: [u8; 4],
}

/// An error returned when parsing fails.
pub enum ParseError {
    /// The input is invalid.
    Invalid,
}
