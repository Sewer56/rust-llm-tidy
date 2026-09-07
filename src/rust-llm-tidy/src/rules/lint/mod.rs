//! Run documentation checks over language facts and measured text.

use crate::reporting::Diagnostic;
pub use crate::rules::registry::*;
pub use crate::text::measurement::{Dialect, DocRegion, RegionLine, line_marker_regions};
pub(crate) use text::is_narration_marker;

pub(crate) mod csharp;
pub(crate) mod rust;
pub(crate) mod text;

/// Check explicit documentation regions in source order.
///
/// # Arguments
///
/// - `regions`: documentation regions with original source line numbers
pub fn run_region_checks(regions: Vec<DocRegion>) -> Vec<Diagnostic> {
    text::diagnostics(&crate::text::measurement::measure(regions))
}

/// Check raw prose or line-marker documentation selected by extension.
///
/// # Arguments
///
/// - `source`: raw prose or commented source
/// - `ext`: extension selecting the line-marker family
pub fn run_text_checks(source: &str, ext: &str) -> Vec<Diagnostic> {
    text::diagnostics(&crate::text::measurement::analyze(source, ext))
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::reporting::Diagnostic;

    /// Select findings for one rule without changing their order.
    pub(crate) fn codes<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diagnostics.iter().filter(|d| d.code == code).collect()
    }
}
