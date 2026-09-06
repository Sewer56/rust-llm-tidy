//! The text rules: TEXT001 and TEXT002 over one measured document,
//! in source order.
//!
//! [`Document`] is the measured input from the plaintext pipeline.
//!
//! [`Document`]: crate::text::measurement::Document

use crate::reporting::diagnostic::Diagnostic;
use crate::text::measurement::Document;

mod text001_paragraph_size;
mod text002_line_length;

/// TEXT001 then TEXT002 diagnostics for one measured document.
///
/// Called by the `run_text_checks` and `run_region_checks` entry points
/// in [`crate::rules::registry`].
pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = text001_paragraph_size::diagnostics(doc);
    diags.extend(text002_line_length::diagnostics(doc));
    diags
}

/// A summary line plus one indented bullet per guidance sentence.
fn bulleted(summary: &str, bullets: &[String]) -> String {
    format!("{summary}\n  - {}", bullets.join("\n  - "))
}
