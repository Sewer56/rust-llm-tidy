//! Reporting boundaries shared by lint rules.

use crate::input::changed_lines::ChangedLines;
use crate::reporting::{Diagnostic, Severity};
use serde::Deserialize;

/// Lines on which a lint may report, independent of whether it is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportingScope {
    /// Report on every eligible line.
    All,
    /// Report only when the diagnostic's line is in the input diff.
    ChangedLines,
}

impl ReportingScope {
    /// Default reporting boundary for a finding's severity.
    pub(crate) fn for_severity(severity: Severity) -> Self {
        match severity {
            Severity::Reminder => Self::ChangedLines,
            _ => Self::All,
        }
    }

    /// Admit only the reported line, never adjacent name or expression lines.
    pub(crate) fn admits(self, diagnostic: &Diagnostic, changed: &ChangedLines) -> bool {
        self == Self::All || changed.overlaps(diagnostic.line, diagnostic.line)
    }
}
