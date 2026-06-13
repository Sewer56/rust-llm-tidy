//! Diagnostic types emitted by documentation checks.
//!
//! A [`Diagnostic`] is a single finding: a severity, a stable rule code, a
//! human-readable message, and a location (1-based line number plus the item
//! kind and name that produced the finding).

/// A single documentation check finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// How severe this finding is.
    pub severity: Severity,
    /// Stable rule code, e.g. `"DOC001"`.
    pub code: &'static str,
    /// Human-readable description of the problem.
    pub message: String,
    /// 1-based line number where the item starts.
    pub line: usize,
    /// The kind of item that produced the finding (e.g. `"fn"`, `"struct"`).
    pub item_kind: String,
    /// The name of the item, if it has one.
    pub item_name: Option<String>,
}

/// Severity of a [`Diagnostic`].
///
/// `Error` severities are gating: a CI run with any `Error` diagnostic should
/// fail. `Warning` severities are advisory and may be surfaced without failing
/// the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    /// A gating finding (missing docs, missing `# Errors` section).
    Error,
    /// An advisory finding (vague error wording).
    Warning,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match &self.item_name {
            Some(name) => write!(
                f,
                "{line}: {sev}[{code}]: {msg} ({kind} `{name}`)",
                line = self.line,
                sev = sev,
                code = self.code,
                msg = self.message,
                kind = self.item_kind,
                name = name,
            ),
            None => write!(
                f,
                "{line}: {sev}[{code}]: {msg} ({kind})",
                line = self.line,
                sev = sev,
                code = self.code,
                msg = self.message,
                kind = self.item_kind,
            ),
        }
    }
}
