//! TEXT010: documentation-context reminder for likely user documentation.
//!
//! Unlike the measured text rules, this check needs file context: a path,
//! its ancestors, and the run's changed-line eligibility.
//!
//! The shared [`super::diagnostics`] dispatcher never emits it, so
//! context-free buffer and region entry points stay silent.
//!
//! Detection lives in `crate::project::documentation`; this module owns
//! only the reminder text so every detection signal shares one message.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_DOCUMENTATION_CONTEXT;

/// Title carried by the reminder, shared by every detection signal.
const TITLE: &str = "documentation audience review";

/// One reminder anchored at `line`, naming the detection `reason`.
///
/// The message asks for an audience review instead of demanding a rewrite:
/// it states what end-user documentation needs, protects reference material
/// and required caveats, and lets suitable content stay unchanged.
pub(crate) fn reminder(line: usize, reason: &str) -> Diagnostic {
    let bullets = [
        "If this is end-user documentation, explain usage and relevant outcomes. Omit internal steps, implementation inventories, and development history unless they change what the reader must do or decide.".to_string(),
        "In a README, prioritize purpose and the shortest useful getting-started path. In a user guide, stay focused on the section's task. Do not expand either into a complete feature or configuration reference.".to_string(),
        "Before retaining a detail, ask: would removing it prevent the reader from completing the task, choosing correctly, or avoiding a meaningful mistake? If not, remove it.".to_string(),
        "If this is explicitly reference or maintainer documentation, preserve the completeness or internal detail its readers need. Still remove repetition and unrelated explanation.".to_string(),
        "Lead with the useful answer or action. Use short paragraphs, direct wording, and focused examples. Link to existing detail rather than repeating it.".to_string(),
        "If a critical prerequisite or warning needs emphasis, use a brief admonition near the section start or before the affected action. Follow supported project syntax; do not repeat the point or add callouts to every section.".to_string(),
        "Prune before reformatting. Do not keep unnecessary content by splitting it into bullets or moving it elsewhere. Preserve required contracts and consequential caveats. Do not invent behavior or guarantees; leave suitable documentation unchanged.".to_string(),
    ];
    Diagnostic {
        title: Some(TITLE.into()),
        severity: Severity::Reminder,
        code: CODE_DOCUMENTATION_CONTEXT,
        message: bulleted(
            &format!(
                "documentation context detected ({reason}).\nReview the changed section for its audience."
            ),
            "Readers need clear guidance for their task, not a description of every underlying behavior.",
            &bullets,
        ),
        line,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The summary names the detected evidence and the review request.
    #[test]
    fn reminder_should_state_the_evidence_and_the_review_request() {
        let finding = reminder(7, "nearby mkdocs.yml");

        assert_eq!(finding.code, CODE_DOCUMENTATION_CONTEXT);
        assert_eq!(finding.severity, Severity::Reminder);
        assert_eq!(finding.line, 7);
        assert_eq!(finding.title(), TITLE);
        assert_eq!(finding.item_kind, "file");
        assert!(finding.message.starts_with(
            "documentation context detected (nearby mkdocs.yml).\nReview the changed section for its audience.\nWhy: "
        ));
    }

    /// Every guidance bullet reaches the message, in order.
    #[test]
    fn reminder_should_keep_the_agreed_guidance() {
        let finding = reminder(1, "file is under docs/");

        for expected in [
            "If this is end-user documentation, explain usage and relevant outcomes.",
            "shortest useful getting-started path",
            "would removing it prevent the reader from completing the task",
            "preserve the completeness or internal detail its readers need",
            "Lead with the useful answer or action.",
            "use a brief admonition near the section start or before the affected action",
            "Prune before reformatting.",
            "leave suitable documentation unchanged",
        ] {
            assert!(finding.message.contains(expected), "missing: {expected}");
        }
    }
}
