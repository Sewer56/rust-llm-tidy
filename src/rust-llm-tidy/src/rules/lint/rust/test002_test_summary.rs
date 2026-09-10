//! `TEST002` - test functions must open with a summary comment.
//!
//! [`check`] fires on test-marked functions with no comment directly above
//! their attribute block. An outer doc run (`///`) or a plain `//` block
//! satisfies it; detection checks presence only, never wording.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_TEST_SUMMARY;
use crate::source::SourceItem;

/// `TEST002` - a test function lacks a summary comment above its attributes.
///
/// Fires on test-marked functions whose attribute block carries no attached
/// comment. `#[ignore]` grants no exemption, and a comment separated by a
/// blank line or sitting below the attributes does not count.
///
/// # Arguments
///
/// - `item`: the parsed source item to check for a leading summary comment.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if !item.is_test_fn() {
        return Vec::new();
    }
    if item.has_summary_comment() {
        return Vec::new();
    }
    let Some(name) = item.name() else {
        return Vec::new();
    };

    vec![Diagnostic {
        title: Some("test missing its summary comment".into()),
        severity: Severity::Reminder,
        code: CODE_TEST_SUMMARY,
        message: format!(
            "test function `{name}` is missing a short explanatory comment above its attributes.\n\n\
             Why: Readers need to understand why this test matters without tracing its body.\n\n\
             Suggestions:\n\
             - Explain the requirement, edge case, or regression the test protects against.\n\
             - Add context rather than restating the test name.\n\
             - Consider Arrange–Act–Assert: setup, action, then assertions, separated by comments."
        ),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: Some(name.to_string()),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::rust::tests::parse_one;
    use rstest::rstest;

    // ── TEST002: test summary comment ──

    // A doc run or a plain comment directly above the attributes satisfies the
    // rule, for every test marker including a skipped test.
    #[rstest]
    #[case::doc_comment("/// Verifies the parser.\n#[test]\nfn parses() {}")]
    #[case::plain_comment("// Verifies the parser.\n#[test]\nfn parses() {}")]
    #[case::multi_line_doc("/// One.\n/// Two.\n#[test]\nfn parses() {}")]
    #[case::doc_block("/** Verifies the parser. */\n#[test]\nfn parses() {}")]
    #[case::nearer_plain_comment("/// Old.\n\n// Verifies the parser.\n#[test]\nfn parses() {}")]
    #[case::skipped_test("// Verifies the parser.\n#[ignore]\n#[test]\nfn parses() {}")]
    #[case::rstest_marker("// Verifies the parser.\n#[rstest]\nfn parses() {}")]
    #[case::test_case_marker("// Verifies the parser.\n#[test_case]\nfn parses() {}")]
    fn check_should_accept_when_a_comment_opens_the_attribute_block(#[case] source: &str) {
        let item = parse_one(source);

        assert!(
            check(&item).is_empty(),
            "a leading comment satisfies TEST002: {source}"
        );
    }

    // A missing comment, a blank-line-separated comment, a comment below the
    // attributes, and an unsummarised skipped test all fire.
    #[rstest]
    #[case::missing("#[test]\nfn parses() {}")]
    #[case::blank_line("// Verifies the parser.\n\n#[test]\nfn parses() {}")]
    #[case::detached_doc("/// Verifies the parser.\n\n#[test]\nfn parses() {}")]
    #[case::detached_doc_block("/** Verifies the parser. */\n\n#[test]\nfn parses() {}")]
    #[case::below_attributes("#[test]\n// Verifies the parser.\nfn parses() {}")]
    #[case::skipped_without_summary("#[ignore]\n#[test]\nfn parses() {}")]
    #[case::rstest_without_summary("#[rstest]\nfn parses() {}")]
    fn check_should_report_when_no_comment_opens_the_attribute_block(#[case] source: &str) {
        let item = parse_one(source);

        let diags = check(&item);

        assert_eq!(diags.len(), 1, "expected one TEST002 finding: {source}");
        assert_eq!(diags[0].code, CODE_TEST_SUMMARY);
        assert_eq!(diags[0].severity, Severity::Reminder);
    }

    // The diagnostic carries the documented message verbatim.
    #[test]
    fn check_should_emit_the_documented_message_when_the_summary_is_missing() {
        let item = parse_one("#[test]\nfn parses() {}");

        let diags = check(&item);

        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "test function `parses` is missing a short explanatory comment above its attributes.\n\n\
             Why: Readers need to understand why this test matters without tracing its body.\n\n\
             Suggestions:\n\
             - Explain the requirement, edge case, or regression the test protects against.\n\
             - Add context rather than restating the test name.\n\
             - Consider Arrange–Act–Assert: setup, action, then assertions, separated by comments."
        );
    }

    // A function without a test marker is not checked.
    #[test]
    fn check_should_skip_when_the_function_is_not_a_test() {
        let item = parse_one("fn helper() {}");

        assert!(check(&item).is_empty());
    }
}
