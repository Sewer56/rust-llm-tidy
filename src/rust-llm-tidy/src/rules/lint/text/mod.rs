//! The text rules: TEXT001 through TEXT007 over one measured document,
//! in source order.
//!
//! [`Document`] is the measured input from the plaintext pipeline.
//!
//! [`Document`]: crate::text::measurement::Document

use crate::reporting::diagnostic::Diagnostic;
use crate::text::measurement::Document;
pub(crate) use text007_passive_narration::is_narration_marker;

mod text001_paragraph_size;
mod text002_line_length;
mod text003_sentence_length;
mod text004_header_opener;
mod text005_fence_tag;
mod text006_verbose_synonyms;
mod text007_passive_narration;

/// TEXT001 through TEXT007 diagnostics for one measured document.
///
/// Called by the `run_text_checks` and `run_region_checks` entry points
/// in [`crate::rules::registry`]. TEXT005 reads the recorded fence
/// facts, so every markdown-prose tier can emit it.
pub(crate) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = text001_paragraph_size::diagnostics(doc);
    diags.extend(text002_line_length::diagnostics(doc));
    diags.extend(text003_sentence_length::diagnostics(doc));
    diags.extend(text004_header_opener::diagnostics(doc));
    diags.extend(text005_fence_tag::diagnostics(doc));
    diags.extend(text006_verbose_synonyms::diagnostics(doc));
    diags.extend(text007_passive_narration::diagnostics(doc));
    diags
}

/// A summary line plus one indented bullet per guidance sentence.
fn bulleted(summary: &str, bullets: &[String]) -> String {
    format!("{summary}\n  - {}", bullets.join("\n  - "))
}
