//! The measured text rules: TEXT001 through TEXT009 over one document,
//! in source order.
//!
//! [`Document`] is the measured input from the plaintext pipeline.
//!
//! TEXT010 is not measured: it classifies files by path, so it lives here
//! only for message ownership; the file pipeline emits it.
//!
//! [`Document`]: crate::text::measurement::Document

use crate::config::forbidden_character_rule::defaults;
use crate::reporting::diagnostic::Diagnostic;
use crate::text::measurement::Document;
pub(crate) use text007_passive_narration::is_narration_marker;
pub(crate) use text010_documentation_context::reminder as documentation_reminder;

pub(crate) mod forbidden_characters;
mod text001_paragraph_size;
mod text002_line_length;
mod text003_sentence_length;
mod text004_header_opener;
mod text005_fence_tag;
mod text006_verbose_synonyms;
mod text007_passive_narration;
mod text008_list_density;
mod text010_documentation_context;

/// TEXT001 through TEXT009 diagnostics for one measured document.
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
    diags.extend(text008_list_density::diagnostics(doc));
    diags.extend(forbidden_characters::diagnostics(doc, defaults()));
    diags
}

/// Format the finding, human-facing reason, and concrete rewrite suggestions.
fn bulleted(summary: &str, why: &str, bullets: &[String]) -> String {
    format!(
        "{summary}\nWhy: {why}\nSuggestions:\n  - {}",
        bullets.join("\n  - ")
    )
}
