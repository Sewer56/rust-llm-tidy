//! The text rules: TEXT001 and TEXT002 over one measured document,
//! in source order.
//!
//! [`Document`] is the measured input from the plaintext pipeline.
//!
//! [`Document`]: crate::check::plaintext::Document

use crate::check::plaintext::Document;
use crate::diagnostic::Diagnostic;

mod line_length;
mod paragraph_length;

/// TEXT001 then TEXT002 diagnostics for one measured document.
///
/// Called by the `run_text_checks` and `run_region_checks` entry points
/// in [`crate::check`].
pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = paragraph_length::diagnostics(doc);
    diags.extend(line_length::diagnostics(doc));
    diags
}

/// A summary line plus one indented bullet per guidance sentence.
fn bulleted(summary: &str, bullets: &[String]) -> String {
    format!("{summary}\n  - {}", bullets.join("\n  - "))
}
