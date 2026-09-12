//! `TEST002` - test methods must open with a summary comment.
//!
//! [`check`] fires on test-marked methods with no comment directly above
//! their attribute list. An XML doc run (`///`) or a plain `//` block
//! satisfies it; detection checks presence only, never wording.

use super::Declaration;
use crate::languages::csharp::parse::{has_leading_comment, has_test_marker};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_TEST_SUMMARY;

/// `TEST002` - a test method lacks a summary comment above its attributes.
///
/// Fires on test-marked methods whose attribute list carries no attached
/// comment. `[Ignore]` grants no exemption, and a comment separated by a
/// blank line or sitting below the attributes does not count.
///
/// # Arguments
///
/// - `decl`: the parsed declaration to check for a leading summary comment.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    if !has_test_marker(decl.node, decl.source) {
        return Vec::new();
    }
    if has_leading_comment(decl.node, decl.source) {
        return Vec::new();
    }
    let Some(name) = decl.name.as_deref() else {
        return Vec::new();
    };

    vec![decl.diagnostic(
        Severity::Reminder,
        CODE_TEST_SUMMARY,
        "test missing its summary comment",
        format!(
            "test method `{name}` is missing a short explanatory comment above its attributes.\n\n\
             Why: Readers need to understand why this test matters without tracing its body.\n\n\
             Suggestions:\n\
             - Explain the requirement, edge case, or regression the test protects against.\n\
             - Add context rather than restating the test name.\n\
             - Consider Arrange–Act–Assert: setup, action, then assertions, separated by comments."
        ),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::analysis::declaration::collect_children;
    use crate::languages::csharp::parse::parse;
    use rstest::rstest;

    /// Parse `source` and return the diagnostics TEST002 produces over every
    /// declaration, driving the shared collector so class members are visible.
    fn check_source(source: &str) -> Vec<Diagnostic> {
        let parsed = parse(source).expect("fixture parses");

        let mut declarations = Vec::new();
        collect_children(
            parsed.syntax_tree().root_node(),
            source,
            None,
            false,
            &mut declarations,
        );

        declarations.iter().flat_map(check).collect()
    }

    // ── TEST002: test summary comment ──

    // An XML doc run or a plain comment directly above the attributes
    // satisfies the rule, for every test marker including a skipped test.
    #[rstest]
    #[case::xml_doc(
        "class C { /// <summary>Verifies the loader.</summary>\n[TestMethod]\npublic void Loads() { } }"
    )]
    #[case::multi_line_xml_doc(
        "class C { /// <summary>One.</summary>\n/// <summary>Two.</summary>\n[TestMethod]\npublic void Loads() { } }"
    )]
    #[case::plain_comment(
        "class C { // Verifies the loader.\n[TestMethod]\npublic void Loads() { } }"
    )]
    #[case::skipped_test(
        "class C { // Verifies the loader.\n[Ignore]\n[TestMethod]\npublic void Loads() { } }"
    )]
    #[case::fact_marker("class C { // Verifies the loader.\n[Fact]\npublic void Loads() { } }")]
    fn check_should_accept_when_a_comment_opens_the_attribute_list(#[case] source: &str) {
        assert!(
            check_source(source).is_empty(),
            "a leading comment satisfies TEST002: {source}"
        );
    }

    // A missing comment, a blank-line-separated comment, a comment below the
    // attributes, and an unsummarised skipped test all fire.
    #[rstest]
    #[case::missing("class C { [TestMethod]\npublic void Loads() { } }")]
    #[case::blank_line(
        "class C { // Verifies the loader.\n\n[TestMethod]\npublic void Loads() { } }"
    )]
    #[case::detached_xml_doc(
        "class C { /// <summary>Verifies the loader.</summary>\n\n[TestMethod]\npublic void Loads() { } }"
    )]
    #[case::below_attributes(
        "class C { [TestMethod]\n// Verifies the loader.\npublic void Loads() { } }"
    )]
    #[case::skipped_without_summary("class C { [Ignore]\n[TestMethod]\npublic void Loads() { } }")]
    fn check_should_report_when_no_comment_opens_the_attribute_list(#[case] source: &str) {
        let diags = check_source(source);

        assert_eq!(diags.len(), 1, "expected one TEST002 finding: {source}");
        assert_eq!(diags[0].code, CODE_TEST_SUMMARY);
        assert_eq!(diags[0].severity, Severity::Reminder);
    }

    // The diagnostic carries the documented message with the C# noun.
    #[test]
    fn check_should_emit_the_documented_message_when_the_summary_is_missing() {
        let diags = check_source("class C { [TestMethod]\npublic void Loads() { } }");

        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "test method `Loads` is missing a short explanatory comment above its attributes.\n\n\
             Why: Readers need to understand why this test matters without tracing its body.\n\n\
             Suggestions:\n\
             - Explain the requirement, edge case, or regression the test protects against.\n\
             - Add context rather than restating the test name.\n\
             - Consider Arrange–Act–Assert: setup, action, then assertions, separated by comments."
        );
    }

    // A method without a test marker is not checked.
    #[test]
    fn check_should_skip_when_the_method_is_not_a_test() {
        let diags = check_source("class C { public void Loads() { } }");

        assert!(diags.is_empty());
    }
}
