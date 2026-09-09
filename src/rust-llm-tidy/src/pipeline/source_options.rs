//! Rule selection for processing source already held in memory.

use crate::config::SymbolRule;

/// Configure standalone source processing without filesystem or subprocess access.
#[derive(Debug, Clone)]
pub struct SourceOptions {
    /// Custom usage regex hints for this buffer; other policies are rejected here.
    pub text_rules: Vec<SymbolRule>,
    /// Symbol hints and declaration exclusions, with the same validation as files.
    /// Text regex hints are also accepted and precede [`Self::text_rules`].
    pub symbol_rules: Vec<SymbolRule>,
    /// Rule or operation whitelist; an empty list uses language defaults.
    pub include: Vec<String>,
    /// Extra rule or operation exclusions.
    pub exclude: Vec<String>,
    /// Minimum repeated inline-link occurrences before hoisting; must be positive.
    pub links_min_occurrences: usize,
    /// Report all lines for every severity, overriding per-rule scopes.
    ///
    /// False respects entry and severity defaults. No input diff is available,
    /// so changed-line findings are hidden.
    ///
    /// Does not enable lints or grant I/O access.
    pub all_lines: bool,
}

impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            text_rules: Vec::new(),
            symbol_rules: Vec::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            links_min_occurrences: 1,
            all_lines: false,
        }
    }
}
