//! Text matching, exact source coordinates, and comment-boundary acceptance tests.

use super::text_regex::check;
use crate::config::{SymbolRule, compile_symbol_rules};
use rstest::rstest;

#[rstest]
#[case::raw_generic("Vec::<u8>::new()", "rs", "'Vec::<u8>::new'", "include", 1)]
#[case::substring("OtherVec::new()", "rs", "'Vec::new'", "include", 1)]
#[case::alternation("OtherVec::new()", "rs", "'Vec::new|Other'", "include", 2)]
#[case::multiline("first\nsecond", "txt", "'(?s)first.*second'", "include", 1)]
#[case::line_anchor("first\nsecond", "txt", "'(?m)^second$'", "include", 1)]
#[case::comment_boundary(
    "first /* comment */ second",
    "js",
    "'(?s)first.*second'",
    "exclude",
    0
)]
#[case::comment_separation("/* first */ /* second */", "js", "'(?s)first.*second'", "only", 0)]
#[case::block_multiline("/* first\nsecond */", "js", "'(?s)first.*second'", "only", 1)]
#[case::delimiters("/* first */", "js", "'/\\* first \\*/'", "only", 1)]
#[case::empty_matches("é", "txt", "'x*'", "include", 2)]
fn hints_should_match_raw_patterns_with_region_boundaries(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] pattern: &str,
    #[case] comments: &str,
    #[case] count: usize,
) {
    let rules = rules(&format!("regex: {pattern}\ncomments: {comments}"));

    let result = check(source, ext, None, &rules);

    assert_eq!(result.hints.len(), count, "{result:?}");
    assert!(result.warnings.is_empty());
}

#[test]
fn hints_should_preserve_original_lines_and_first_rule_at_same_start() {
    let source = "// é\r\nlet x = 'needle'; /* needle\r\nneedle */";
    let mut policies = rules("regex: needle\ncomments: only");
    let mut second = rules("regex: needle\ncomments: include");
    second[0].message = Some("second".into());
    policies.extend(second);

    let result = check(source, "js", None, &policies);

    let matches: Vec<_> = result
        .hints
        .iter()
        .map(|hint| {
            (
                hint.diagnostic.line,
                hint.diagnostic.message.as_str(),
                hint.diagnostic.item_name.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        matches,
        [
            (2, "second", Some("needle")),
            (2, "first", Some("needle")),
            (3, "first", Some("needle"))
        ]
    );
}

#[rstest]
#[case::selected("MD", 1)]
#[case::not_selected("js", 0)]
fn hints_should_respect_case_insensitive_text_extensions(#[case] ext: &str, #[case] count: usize) {
    let rules = rules("regex: needle\nextensions: [md]");

    let result = check("needle", ext, None, &rules);

    assert_eq!(result.hints.len(), count);
}

#[rstest]
#[case::rust("fn f() { let x = \"needle\"; } // needle", "rs", "include", 2)]
#[case::rust_exclude("fn f() { let x = \"needle\"; } // needle", "rs", "exclude", 1)]
#[case::rust_only("fn f() { let x = \"needle\"; } // needle", "rs", "only", 1)]
#[case::csharp("class C { string s = \"needle\"; /* needle */ }", "cs", "exclude", 1)]
#[case::python("s = 'needle' # needle", "py", "exclude", 1)]
#[case::python_stub("s = 'needle' # needle", "pyi", "only", 1)]
#[case::javascript("let s = 'needle'; // needle", "js", "exclude", 1)]
#[case::ruby("s = 'needle' # needle", "rb", "only", 1)]
#[case::sql("select 'needle'; -- needle", "sql", "exclude", 1)]
#[case::lisp("(print \"needle\") ; needle", "el", "only", 1)]
#[case::tex("needle % needle", "tex", "exclude", 1)]
#[case::yaml("s: needle # needle", "yaml", "only", 1)]
#[case::markdown("needle in prose", "md", "include", 1)]
#[case::plaintext("needle in prose", "txt", "include", 1)]
#[case::unparsed_include("const x = /needle/;", "js", "include", 1)]
fn hints_should_search_selected_text(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] comments: &str,
    #[case] count: usize,
) {
    let rules = rules(&format!("regex: needle\ncomments: {comments}"));

    let result = check(source, ext, None, &rules);

    assert_eq!(result.hints.len(), count, "{result:?}");
    assert!(result.warnings.is_empty(), "{result:?}");
}

#[rstest]
#[case::syntax_error("fn f( { // needle", "rs")]
#[case::python_error("s = '''needle", "py")]
#[case::raw_string("auto s = R\"(needle)\";", "cpp")]
#[case::slash_literal("const x = /needle/;", "js")]
#[case::unterminated_quote("const x = 'needle", "js")]
#[case::nested_comment("/* /* needle */ */", "swift")]
#[case::yaml_scalar("s: |\n  needle", "yaml")]
#[case::no_comment_parser("needle <!-- needle -->", "md")]
fn hints_should_warn_and_skip_only_comment_sensitive_rules(
    #[case] source: &str,
    #[case] ext: &str,
    #[values("exclude", "only")] comments: &str,
) {
    let mut policies = rules(&format!("regex: needle\ncomments: {comments}"));
    policies.extend(rules("regex: needle\ncomments: include"));

    let result = check(source, ext, None, &policies);

    assert!(!result.hints.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("comments cannot be reliably identified"));
}

/// Compile one hint through the same validator as configuration loading.
fn rules(fields: &str) -> Vec<crate::config::CompiledSymbolRule> {
    let rule: SymbolRule =
        serde_yml::from_str(&format!("title: Review\nmessage: first\n{fields}")).unwrap();
    compile_symbol_rules(&[rule]).unwrap()
}
