//! Diagnostic types emitted by documentation checks.
//!
//! A [`Diagnostic`] is a single finding: a severity, a stable rule code, a
//! human-readable message, and a location (1-based line number plus the item
//! kind and name that produced the finding).

use crate::check::title_for_code;

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

impl Diagnostic {
    /// Friendly title for this finding's rule code, e.g.
    /// `"missing documentation"` for `DOC001`.
    ///
    /// Falls back to the raw code when the code has no title.
    pub fn title(&self) -> &'static str {
        title_for_code(self.code).unwrap_or(self.code)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::CODE_MISSING_DOCS;

    /// Minimal finding carrying `code`; only `title()` reads the fields.
    fn diagnostic(code: &'static str) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            code,
            message: String::new(),
            line: 1,
            item_kind: "fn".to_string(),
            item_name: None,
        }
    }

    /// A known code resolves to its friendly title.
    #[test]
    fn title_returns_the_friendly_title_for_a_known_code() {
        assert_eq!(
            diagnostic(CODE_MISSING_DOCS).title(),
            "missing documentation"
        );
    }

    /// A code with no title entry resolves to the raw code.
    #[test]
    fn title_falls_back_to_the_raw_code_when_untitled() {
        assert_eq!(diagnostic("DOC999").title(), "DOC999");
    }
}
