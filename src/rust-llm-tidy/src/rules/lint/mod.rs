//! Run documentation, size, and duplication checks over facts and source text.

use crate::reporting::Diagnostic;
pub use crate::rules::registry::*;
use crate::text::measurement;
pub use crate::text::measurement::{Dialect, DocRegion, RegionLine, line_marker_regions};
pub(crate) use text::is_narration_marker;

pub(crate) mod csharp;
pub(crate) mod doc009_missing_module_docs;
pub(crate) mod dup001_duplication;
pub(crate) mod mod001_module_size;
pub(crate) mod rust;
pub(crate) mod symbols;
pub(crate) mod text;

/// Check explicit documentation regions in source order.
///
/// # Arguments
///
/// - `regions`: documentation regions with original source line numbers
pub fn run_region_checks(regions: Vec<DocRegion>) -> Vec<Diagnostic> {
    text::diagnostics(&measurement::measure(regions))
}

/// Check raw prose or line-marker documentation selected by extension.
///
/// # Arguments
///
/// - `source`: raw prose or commented source
/// - `ext`: extension selecting the line-marker family
pub fn run_text_checks(source: &str, ext: &str) -> Vec<Diagnostic> {
    text::diagnostics(&measurement::analyze(source, ext))
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::reporting::Diagnostic;

    /// Select findings for one rule without changing their order.
    pub(crate) fn codes<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diagnostics.iter().filter(|d| d.code == code).collect()
    }
}
