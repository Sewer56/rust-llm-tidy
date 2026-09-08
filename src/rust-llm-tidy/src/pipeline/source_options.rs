//! Rule selection for processing source already held in memory.

use crate::config::ReportingScope;

/// Configure standalone source processing without filesystem or subprocess access.
#[derive(Debug, Clone)]
pub struct SourceOptions {
    /// Rule or operation whitelist; an empty list uses language defaults.
    pub include: Vec<String>,
    /// Extra rule or operation exclusions.
    pub exclude: Vec<String>,
    /// Minimum repeated inline-link occurrences before hoisting; must be positive.
    pub links_min_occurrences: usize,
    /// Override severity-based reporting without granting Git access.
    /// No input diff is available: reminders are hidden unless set to `All`.
    pub lint_scope: Option<ReportingScope>,
}

impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            links_min_occurrences: 1,
            lint_scope: None,
        }
    }
}
