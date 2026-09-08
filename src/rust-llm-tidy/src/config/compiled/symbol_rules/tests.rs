//! Acceptance tests for symbol policy validation and bounded compilation.

use super::*;
use crate::config::Config;
use crate::config::compiled::load::compile;
use rstest::rstest;

#[test]
fn config_should_expose_compiled_policies_independently_of_enablement() {
    let config = compile(
        "exclude: [{rules: [SYM001]}]\nsymbol_rules: [{symbol: Generated, target: declaration, action: exclude}]",
        &[],
    );

    let rules = config.symbol_rules();

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].action, SymbolAction::Exclude);
}

#[rstest]
#[case::unknown_language("symbol_rules: [{symbol: x, message: hi, language: python}]")]
#[case::unknown_target("symbol_rules: [{symbol: x, message: hi, target: reference}]")]
#[case::unknown_action("symbol_rules: [{symbol: x, message: hi, action: warn}]")]
#[case::unknown_field("symbol_rules: [{symbol: x, message: hi, typo: all}]")]
#[case::unknown_scope("lint_scopes: {SYM001: changed}")]
#[case::unknown_array_kind("symbol_rules: [{symbol: 'new[]', message: hi, array_kind: vector}]")]
fn config_should_reject_unknown_symbol_values(#[case] yaml: &str) {
    let result = serde_yml::from_str::<Config>(yaml);

    assert!(result.is_err());
}

#[rstest]
#[case::expanded_program("(?:a{1000}){1000}")]
#[case::deep_nesting(&format!("{}x{}", "(".repeat(REGEX_NEST_LIMIT as usize), ")".repeat(REGEX_NEST_LIMIT as usize)))]
fn regex_should_reject_excessive_compilation_work(#[case] pattern: &str) {
    let rule: SymbolRule =
        serde_yml::from_str(&format!("regex: '{pattern}'\nmessage: reminder")).unwrap();

    let result = compile_symbol_rules(&[rule]);

    assert!(result.is_err());
}

#[rstest]
#[case::at_limit(MAX_SYMBOL_RULES, true)]
#[case::over_limit(MAX_SYMBOL_RULES + 1, false)]
fn rules_should_bound_rule_count(#[case] count: usize, #[case] valid: bool) {
    let rule: SymbolRule = serde_yml::from_str("symbol: x\nmessage: reminder").unwrap();
    let rules = vec![rule; count];

    let result = compile_symbol_rules(&rules);

    assert_eq!(result.is_ok(), valid);
}

#[rstest]
#[case::pattern_at_limit("symbol", MAX_PATTERN_BYTES, true)]
#[case::pattern_over_limit("symbol", MAX_PATTERN_BYTES + 1, false)]
#[case::regex_at_limit("regex", MAX_PATTERN_BYTES, true)]
#[case::regex_over_limit("regex", MAX_PATTERN_BYTES + 1, false)]
#[case::message_at_limit("message", MAX_MESSAGE_BYTES, true)]
#[case::message_over_limit("message", MAX_MESSAGE_BYTES + 1, false)]
fn rules_should_bound_text_before_compilation(
    #[case] field: &str,
    #[case] length: usize,
    #[case] valid: bool,
) {
    let value = "x".repeat(length);
    let yaml = if field == "message" {
        format!("symbol: x\nmessage: {value}")
    } else {
        format!("{field}: {value}\nmessage: reminder")
    };
    let rule: SymbolRule = serde_yml::from_str(&yaml).unwrap();

    let result = compile_symbol_rules(&[rule]);

    assert_eq!(result.is_ok(), valid, "{result:?}");
}

#[rstest]
#[case::literal("symbol: Vec::new\nmessage: reminder")]
#[case::array("symbol: new[]\nmessage: reminder\narray_kind: any\nextensions: [CS, rs, Cs]")]
#[case::regex("regex: 'Vec::.*'\nmessage: reminder")]
#[case::declaration("symbol: NotInAnyFile\ntarget: declaration\nmessage: reminder")]
#[case::exclusion("symbol: Generated\ntarget: declaration\naction: exclude")]
fn rules_should_compile_without_source_matches(#[case] entry: &str) {
    let rule: SymbolRule = serde_yml::from_str(entry).unwrap();

    let compiled = compile_symbol_rules(&[rule]).unwrap();

    assert_eq!(compiled.len(), 1);
}

#[rstest]
#[case::neither("message: reminder", "exactly one")]
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
#[case::invalid_regex("regex: '['\nmessage: reminder", "bounded compilation")]
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
    let rule: SymbolRule = serde_yml::from_str(entry).unwrap();

    let error = compile_symbol_rules(&[rule]).unwrap_err();

    assert!(format!("{error:#}").contains(expected), "{error:#}");
}

#[rstest]
#[case::symbol_default("{}", "SYM001", None)]
#[case::performance_default("{}", "PERF001", None)]
#[case::documentation_default("{}", "DOC001", None)]
#[case::symbol_override("lint_scopes: {SYM001: all}", "SYM001", Some(ReportingScope::All))]
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
