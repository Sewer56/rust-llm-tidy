//! Acceptance tests for trusted policies and default regex compilation.

use super::*;
use crate::config::Config;
use crate::config::compiled::load::compile;
use rstest::rstest;

#[test]
fn config_should_expose_compiled_policies_independently_of_enablement() {
    let config = compile(
        "exclude: [{rules: [SYM]}]\nsymbol_rules: [{symbol: Generated, target: declaration, action: exclude}]",
        &[],
    );

    let rules = config.symbol_rules();

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].action, SymbolAction::Exclude);
}

#[rstest]
#[case::unknown_language(
    "symbol_rules: [{symbol: x, title: API, message: hi, languages: [python]}]"
)]
#[case::removed_language("symbol_rules: [{symbol: x, title: API, message: hi, language: rust}]")]
#[case::unknown_target("symbol_rules: [{symbol: x, message: hi, target: reference}]")]
#[case::unknown_action("symbol_rules: [{symbol: x, message: hi, action: warn}]")]
#[case::unknown_field("symbol_rules: [{symbol: x, message: hi, typo: all}]")]
#[case::unknown_scope("lint_scopes: {SYM: changed}")]
#[case::unknown_array_kind("symbol_rules: [{symbol: 'new[]', message: hi, array_kind: vector}]")]
#[case::lint_scalar("symbol_rules: [{symbol: x, exclude_lints: DOC001}]")]
#[case::lint_map("symbol_rules: [{symbol: x, exclude_lints: {DOC001: true}}]")]
#[case::lint_number("symbol_rules: [{symbol: x, exclude_lints: 1}]")]
#[case::lint_boolean_list("symbol_rules: [{symbol: x, exclude_lints: [true]}]")]
#[case::edit_list("symbol_rules: [{symbol: x, exclude_edits: [reorder]}]")]
#[case::post_process_list("symbol_rules: [{symbol: x, exclude_post_process: [rustfmt]}]")]
fn config_should_reject_unknown_symbol_values(#[case] yaml: &str) {
    let result = serde_yml::from_str::<Config>(yaml);

    assert!(result.is_err());
}

#[rstest]
#[case::unicode_class(r"\w", "word")]
#[case::expanded_program(r"\w{2}", "ok")]
#[case::nested_groups(&format!("{}x{}", "(".repeat(96), ")".repeat(96)), "x")]
fn regex_should_match_with_dependency_defaults(#[case] pattern: &str, #[case] source: &str) {
    let rule: SymbolRule = serde_yml::from_str(&format!(
        "regex: '{pattern}'\ntitle: API\nmessage: reminder"
    ))
    .unwrap();

    let compiled = compile_symbol_rules(&[rule]).unwrap();
    let SymbolMatcher::Regex(actual) = &compiled[0].matcher else {
        panic!("expected a regex matcher");
    };
    let expected = Regex::new(pattern).unwrap();

    let ranges = |regex: &Regex| {
        regex
            .find_iter(source)
            .map(|found| found.range())
            .collect::<Vec<_>>()
    };
    assert!(!ranges(&expected).is_empty());
    assert_eq!(ranges(actual), ranges(&expected));
}

#[rstest]
#[case::expanded_program(r"\w{1000}")]
#[case::nested_groups(&format!("{}x{}", "(".repeat(300), ")".repeat(300)))]
fn regex_should_retain_dependency_compilation_errors(#[case] pattern: &str) {
    let rule: SymbolRule = serde_yml::from_str(&format!(
        "regex: '{pattern}'\ntitle: API\nmessage: reminder"
    ))
    .unwrap();
    let expected = Regex::new(pattern).unwrap_err();

    let error = compile_symbol_rules(&[rule]).unwrap_err();

    assert_eq!(error.root_cause().to_string(), expected.to_string());
}

#[rstest]
#[case::literal("symbol: Vec::new\ntitle: API\nmessage: reminder")]
#[case::array(
    "symbol: new[]\ntitle: Array\nmessage: reminder\narray_kind: any\nextensions: [CS, rs, Cs]"
)]
#[case::regex("regex: 'Vec::.*'\ntitle: API\nmessage: reminder")]
#[case::text_extensions("regex: text\ntitle: API\nmessage: reminder\nextensions: [MD, js, py]")]
#[case::declaration("symbol: NotInAnyFile\ntarget: declaration\ntitle: API\nmessage: reminder")]
#[case::exclusion("symbol: Generated\ntarget: declaration\naction: exclude")]
fn rules_should_compile_without_source_matches(#[case] entry: &str) {
    let rule: SymbolRule = serde_yml::from_str(entry).unwrap();

    let compiled = compile_symbol_rules(&[rule]).unwrap();

    assert_eq!(compiled.len(), 1);
}

#[test]
fn rules_should_preserve_large_configured_lists() {
    let rule: SymbolRule = serde_yml::from_str("symbol: x\ntitle: API\nmessage: reminder").unwrap();
    let rules = vec![rule; 512];

    let compiled = compile_symbol_rules(&rules).unwrap();

    assert_eq!(compiled.len(), rules.len());
}

#[rstest]
#[case::symbol("symbol")]
#[case::regex("regex")]
#[case::message("message")]
#[case::title("title")]
fn rules_should_preserve_long_nonblank_text(#[case] field: &str) {
    let value = "x".repeat(32 * 1024);
    let yaml = if field == "message" {
        format!("symbol: x\ntitle: API\nmessage: {value}")
    } else if field == "title" {
        format!("symbol: x\ntitle: {value}\nmessage: reminder")
    } else {
        format!("{field}: {value}\ntitle: API\nmessage: reminder")
    };
    let rule: SymbolRule = serde_yml::from_str(&yaml).unwrap();

    let compiled = compile_symbol_rules(&[rule]).unwrap();

    let actual = match field {
        "message" => compiled[0].message.as_deref().unwrap(),
        "title" => compiled[0].title.as_deref().unwrap(),
        _ => match &compiled[0].matcher {
            SymbolMatcher::Literal(symbol) => symbol,
            SymbolMatcher::Regex(regex) => regex.as_str(),
        },
    };
    assert_eq!(actual, value);
}

#[rstest]
#[case::neither("message: reminder", "exactly one")]
#[case::hint_lints(
    "symbol: x\nmessage: hi\nexclude_lints: false",
    "require action: exclude"
)]
#[case::hint_edits(
    "symbol: x\nmessage: hi\nexclude_edits: false",
    "require action: exclude"
)]
#[case::hint_post_process_false(
    "symbol: x\nmessage: hi\nexclude_post_process: false",
    "require action: exclude"
)]
#[case::hint_post_process_true(
    "symbol: x\nmessage: hi\nexclude_post_process: true",
    "require action: exclude"
)]
#[case::declaration_hint_post_process(
    "symbol: x\ntarget: declaration\nmessage: hi\nexclude_post_process: false",
    "require action: exclude"
)]
#[case::text_hint_post_process(
    "regex: x\nmessage: hi\nexclude_post_process: true",
    "require action: exclude"
)]
#[case::declaration_hint_lints(
    "symbol: x\ntarget: declaration\nmessage: hi\nexclude_lints: []",
    "require action: exclude"
)]
#[case::text_hint_edits(
    "regex: x\nmessage: hi\nexclude_edits: true",
    "require action: exclude"
)]
#[case::unknown_lint(
    "symbol: x\ntarget: declaration\naction: exclude\nexclude_lints: [DOC999]",
    "unknown lint code"
)]
#[case::retired_lint(
    "symbol: x\ntarget: declaration\naction: exclude\nexclude_lints: [DOC007]",
    "unknown lint code"
)]
#[case::operation(
    "symbol: x\ntarget: declaration\naction: exclude\nexclude_lints: [reorder]",
    "unknown lint code"
)]
#[case::performance_family(
    "symbol: x\ntarget: declaration\naction: exclude\nexclude_lints: [PERF001]",
    "unknown lint code"
)]
#[case::lowercase_lint(
    "symbol: x\ntarget: declaration\naction: exclude\nexclude_lints: [doc001]",
    "unknown lint code"
)]
#[case::regex_languages(
    "regex: x\nlanguages: [rust]\nmessage: hi",
    "languages cannot restrict"
)]
#[case::regex_arguments("regex: x\nzero_arguments: true\nmessage: hi", "text regexes cannot")]
#[case::regex_initializer("regex: x\nno_initializer: true\nmessage: hi", "text regexes cannot")]
#[case::regex_array("regex: x\narray_kind: any\nmessage: hi", "text regexes cannot")]
#[case::symbol_comments("symbol: x\ncomments: include\nmessage: hi", "comments is only")]
#[case::declaration_comments(
    "regex: x\ntarget: declaration\ncomments: only\nmessage: hi",
    "comments is only"
)]
#[case::regex_unknown_extension(
    "regex: x\nextensions: [unknown]\nmessage: hi",
    "supported text extensions"
)]
#[case::empty_languages("symbol: x\nmessage: hi\nlanguages: []", "languages must not be empty")]
#[case::empty_extensions("symbol: x\nmessage: hi\nextensions: []", "must not be empty")]
#[case::dotted_extension("symbol: x\nmessage: hi\nextensions: [.cs]", "without leading dots")]
#[case::unknown_extension("symbol: x\nmessage: hi\nextensions: [py]", "rs or cs")]
#[case::invalid_extension("symbol: x\nmessage: hi\nextensions: ['c/s']", "rs or cs")]
#[case::declaration_array(
    "symbol: x\ntarget: declaration\nmessage: hi\narray_kind: any",
    "declaration rules"
)]
#[case::both("symbol: x\nregex: x\nmessage: reminder", "exactly one")]
#[case::blank_symbol("symbol: ' '\nmessage: reminder", "symbol must not be empty")]
#[case::empty_component("symbol: x::::y\nmessage: reminder", "literal components")]
#[case::wildcard_literal("symbol: 'x*'\nmessage: reminder", "literal components")]
#[case::blank_regex("regex: ''\nmessage: reminder", "regex must not be empty")]
#[case::invalid_regex("regex: '['\nmessage: reminder", "regex compilation failed")]
#[case::missing_message("symbol: x", "message must not be empty")]
#[case::blank_message("symbol: x\nmessage: ' '", "message must not be empty")]
#[case::usage_exclusion("symbol: x\naction: exclude", "exclude requires")]
#[case::exclusion_message(
    "symbol: x\ntarget: declaration\naction: exclude\nmessage: hi",
    "exclude requires"
)]
#[case::exclusion_scope(
    "symbol: x\ntarget: declaration\naction: exclude\nscope: all",
    "exclude requires"
)]
#[case::declaration_arguments(
    "symbol: x\ntarget: declaration\nmessage: hi\nzero_arguments: true",
    "declaration rules"
)]
#[case::declaration_initializer(
    "symbol: x\ntarget: declaration\nmessage: hi\nno_initializer: false",
    "declaration rules"
)]
fn rules_should_reject_incompatible_fields(#[case] entry: &str, #[case] expected: &str) {
    let entry = if entry.contains("action: exclude") {
        entry.to_owned()
    } else {
        format!("title: API\n{entry}")
    };
    let rule: SymbolRule = serde_yml::from_str(&entry).unwrap();

    let error = compile_symbol_rules(&[rule]).unwrap_err();

    assert!(format!("{error:#}").contains(expected), "{error:#}");
}

#[rstest]
#[case::missing("symbol: x\nmessage: hi")]
#[case::empty("symbol: x\nmessage: hi\ntitle: ''")]
#[case::blank("symbol: x\nmessage: hi\ntitle: '  '")]
#[case::exclusion("symbol: x\ntarget: declaration\naction: exclude\ntitle: API")]
fn rules_should_reject_missing_blank_or_exclusion_titles(#[case] entry: &str) {
    let rule: SymbolRule = serde_yml::from_str(entry).unwrap();

    let error = compile_symbol_rules(&[rule]).unwrap_err();

    assert!(format!("{error:#}").contains("title"));
}

#[rstest]
#[case::symbol_default("{}", "SYM", None)]
#[case::documentation_default("{}", "DOC001", None)]
#[case::symbol_override("lint_scopes: {SYM: all}", "SYM", Some(ReportingScope::All))]
#[case::documentation_override(
    "lint_scopes: {DOC001: changed_lines}",
    "DOC001",
    Some(ReportingScope::ChangedLines)
)]
fn scope_should_preserve_explicit_code_overrides(
    #[case] yaml: &str,
    #[case] code: &str,
    #[case] expected: Option<ReportingScope>,
) {
    let config = compile(yaml, &[]);

    let scope = config.scope_for(code);

    assert_eq!(scope, expected);
}
