//! Diagnostic types emitted by documentation checks.
//!
//! A [`Diagnostic`] is a single finding: a severity, a stable rule code, a
//! human-readable message, and a location. The location is a 1-based line
//! number plus the item kind and name that produced the finding.

use crate::rules::registry::{CODE_FORBIDDEN_CHARACTERS, CODE_SYM};
use core::fmt;
use serde::Deserialize;

/// A single documentation check finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// How severe this finding is.
    pub severity: Severity,
    /// Stable rule code, e.g. `"DOC001"`.
    pub code: &'static str,
    /// Producer-owned title; absent falls back to `code` in [`Self::title`].
    ///
    /// Plaintext prefixes `SYM` and `TEXT009` messages with this title; other
    /// diagnostics already carry a complete finding summary in `message`.
    pub title: Option<Box<str>>,
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
///
/// `Hint` severities are suggestions for a large language model or a human
/// to investigate, such as a possible pre-allocation. They never fail a run
/// and surface separately from errors and warnings.
///
/// `Reminder` findings are conditional guidance for humans and AI, not proven
/// defects. `AiReminder` findings are guidance for AI language models only,
/// such as steering toward an optimal repair.
///
/// Both reminder severities report only on changed input lines by default.
/// Errors, warnings, and hints default to whole-file reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// A gating finding (missing docs, missing `# Errors` section).
    Error,
    /// An advisory finding (vague error wording).
    Warning,
    /// A suggestion for an LLM or human to investigate; see the enum
    /// documentation for gating and compatibility.
    Hint,
    /// Non-gating guidance for humans and AI, reported on changed input lines
    /// by default.
    Reminder,
    /// Non-gating guidance for AI language models only, reported on changed
    /// input lines by default.
    AiReminder,
}

impl Diagnostic {
    /// Producer-owned title, or the raw code when no title was supplied.
    pub fn title(&self) -> &str {
        self.title.as_deref().unwrap_or(self.code)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "hint",
            Severity::Reminder => "reminder",
            Severity::AiReminder => "ai_reminder",
        };
        let title = self
            .title
            .as_deref()
            .filter(|_| matches!(self.code, CODE_SYM | CODE_FORBIDDEN_CHARACTERS));
        let separator = if title.is_some() { ": " } else { "" };
        let title = title.unwrap_or_default();

        match &self.item_name {
            Some(name) => write!(
                f,
                "{line}: {sev}[{code}]: {title}{separator}{msg} ({kind} `{name}`)",
                line = self.line,
                sev = sev,
                code = self.code,
                msg = self.message,
                kind = self.item_kind,
                name = name,
            ),
            None => write!(
                f,
                "{line}: {sev}[{code}]: {title}{separator}{msg} ({kind})",
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
    use crate::rules::registry::CODE_MISSING_DOCS;

    /// Minimal finding carrying `code`; only `title()` reads the fields.
    fn diagnostic(code: &'static str) -> Diagnostic {
        Diagnostic {
            title: None,
            severity: Severity::Warning,
            code,
            message: String::new(),
            line: 1,
            item_kind: "fn".to_string(),
            item_name: None,
        }
    }

    #[rstest::rstest]
    #[case::known(CODE_MISSING_DOCS)]
    #[case::unknown("DOC999")]
    fn title_should_return_raw_code_when_untitled(#[case] code: &'static str) {
        assert_eq!(diagnostic(code).title(), code);
    }

    #[test]
    fn title_should_return_producer_title() {
        let mut finding = diagnostic(CODE_MISSING_DOCS);
        finding.title = Some("producer title".into());

        assert_eq!(finding.title(), "producer title");
    }

    /// A hint-severity finding renders with the `hint` severity token in
    /// the shared plaintext line shape.
    #[test]
    fn display_renders_hint_severity() {
        let finding = Diagnostic {
            title: None,
            severity: Severity::Hint,
            code: "DOC999",
            message: "consider pre-allocating the buffer".to_string(),
            line: 3,
            item_kind: "fn".to_string(),
            item_name: Some("load".to_string()),
        };

        assert_eq!(
            finding.to_string(),
            "3: hint[DOC999]: consider pre-allocating the buffer (fn `load`)"
        );
    }
}
