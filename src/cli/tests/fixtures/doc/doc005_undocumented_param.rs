//! Rule: DOC005 - every parameter must be mentioned in the `# Arguments` section.
//!
//! When a `# Arguments` section exists, each parameter name should appear
//! somewhere in it. Parameters that are not mentioned are flagged.
//!
//! Expected diagnostics:
//! - DOC005 on `pub fn build` (param `fmt` is not mentioned)
//!
//! Not flagged (should pass):
//! - `pub fn render` (both `text` and `width` are documented)

/// Builds output from a name and format.
///
/// # Arguments
///
/// `name` - the name to use.
pub fn build(name: &str, fmt: &str) -> String {
    format!("{name} {fmt}")
}

/// Renders text at a given width.
///
/// # Arguments
///
/// `text` - the text to render.
/// `width` - the target width.
pub fn render(text: &str, width: u32) -> String {
    format!("{text} {width}")
}
