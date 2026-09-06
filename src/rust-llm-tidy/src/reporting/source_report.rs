//! Transformed source and findings for a standalone buffer.

use super::{Change, Diagnostic};
use std::borrow::Cow;

/// Standalone processing output, borrowing the original source when unchanged.
#[derive(Debug)]
pub struct SourceReport<'a> {
    /// Final source after enabled transformations.
    pub source: Cow<'a, str>,
    /// Changes in transformation order; line anchors refer to each pass's input.
    pub changes: Vec<Change>,
    /// Lint findings against the final transformed source.
    pub diagnostics: Vec<Diagnostic>,
}
