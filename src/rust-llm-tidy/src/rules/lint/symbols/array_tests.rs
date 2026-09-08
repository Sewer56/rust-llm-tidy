//! Array syntax and extension selection acceptance through shared symbol policies.

use super::*;
use crate::config::{SymbolRule, compile_symbol_rules};
use crate::languages::backend_for;
use rstest::rstest;

#[rstest]
#[case::literal("symbol: new[]")]
#[case::regex("regex: 'new\\[\\]'")]
fn array_hints_should_anchor_to_new_and_preserve_nested_calls(#[case] matcher: &str) {
    let source = "class C { void M() { var a = new\n int[Size()]; } }";
    let parsed = backend_for("cs").unwrap().parse(source).unwrap();
    let yaml = format!("- {matcher}\n  message: array\n- symbol: Size\n  message: callback");
    let rules: Vec<SymbolRule> = serde_yml::from_str(&yaml).unwrap();

    let result = check(&parsed, "cs", &compile_symbol_rules(&rules).unwrap()).unwrap();

    assert_eq!(result.hints.len(), 2);
    assert_eq!(
        result.hints[0].diagnostic.item_name.as_deref(),
        Some("new[]")
    );
    assert_eq!(result.hints[0].diagnostic.line, 1);
    assert_eq!(
        result.hints[1].diagnostic.item_name.as_deref(),
        Some("Size")
    );
    assert_eq!(result.hints[1].diagnostic.line, 2);
}

#[rstest]
#[case::sized("new int[3]", "explicit_sized_vector", true, true, 1)]
#[case::initializer("new int[1] { 1 }", "explicit_sized_vector", false, true, 1)]
#[case::reject_initializer("new int[1] { 1 }", "explicit_sized_vector", true, true, 0)]
#[case::reject_implicit("new[] { 1 }", "explicit_sized_vector", false, true, 0)]
#[case::any_implicit("new[] { 1 }", "any", false, true, 1)]
#[case::reject_multidimensional("new int[2, 3]", "explicit_sized_vector", true, true, 0)]
#[case::reject_jagged("new int[3][]", "explicit_sized_vector", true, true, 0)]
#[case::sizes_not_arguments("new int[3]", "any", true, false, 0)]
fn array_hints_should_apply_custom_shape_and_usage_constraints(
    #[case] expression: &str,
    #[case] kind: &str,
    #[case] no_initializer: bool,
    #[case] zero_arguments: bool,
    #[case] count: usize,
) {
    let source = format!("class C {{ void M() {{ var a = {expression}; }} }}");
    let parsed = backend_for("cs").unwrap().parse(&source).unwrap();
    let yaml = format!(
        "- symbol: new[]\n  message: custom\n  array_kind: {kind}\n  \
         no_initializer: {no_initializer}\n  zero_arguments: {zero_arguments}"
    );
    let rules: Vec<SymbolRule> = serde_yml::from_str(&yaml).unwrap();

    let result = check(&parsed, "cs", &compile_symbol_rules(&rules).unwrap()).unwrap();

    assert_eq!(result.hints.len(), count, "{result:?}");
}

#[rstest]
#[case::sized("new int[Size()]", 1, 1)]
#[case::generic("new List<int>[4]", 1, 1)]
#[case::comment("new int[/* capacity */ 4]", 1, 1)]
#[case::explicit_initializer("new int[] { 1 }", 1, 0)]
#[case::sized_initializer("new int[1] { 1 }", 1, 0)]
#[case::implicit("new[] { 1 }", 1, 0)]
#[case::multidimensional("new int[2, 3]", 1, 0)]
#[case::implicit_multidimensional("new[,] { { 1, 2 } }", 1, 0)]
#[case::jagged("new int[3][]", 1, 0)]
#[case::nested("new[] { new int[3] }", 2, 1)]
#[case::object("new List<int>()", 0, 0)]
#[case::implicit_object("new()", 0, 0)]
#[case::stackalloc("stackalloc int[3]", 0, 0)]
#[case::collection("[1, 2]", 0, 0)]
#[case::string("\"new int[3]\"", 0, 0)]
fn array_hints_should_select_supported_shapes(
    #[case] expression: &str,
    #[case] custom_count: usize,
    #[case] builtin_count: usize,
) {
    let source = format!("class C {{ void M() {{ var a = {expression}; }} }}");
    let parsed = backend_for("cs").unwrap().parse(&source).unwrap();
    let rules: Vec<SymbolRule> = serde_yml::from_str(
        "- symbol: new[]\n  array_kind: any\n  extensions: [cS]\n  message: custom",
    )
    .unwrap();
    let rules = compile_symbol_rules(&rules).unwrap();

    let custom = check(&parsed, "CS", &rules).unwrap();
    let builtin = check(&parsed, "cs", &[builtins::array_reminder()]).unwrap();

    assert_eq!(custom.hints.len(), custom_count, "{custom:?}");
    assert_eq!(builtin.hints.len(), builtin_count, "{builtin:?}");
}

#[rstest]
#[case::rust_enabled("fn f() {}", "RS", "extensions: [rS]", 1)]
#[case::rust_disabled("fn f() {}", "rs", "extensions: [CS]", 0)]
#[case::csharp_enabled("class C {}", "Cs", "extensions: [cS]", 1)]
#[case::both("class C {}", "cs", "extensions: [rs, cs]", 1)]
#[case::intersection("class C {}", "cs", "extensions: [cs]\n  language: rust", 0)]
fn rules_should_intersect_extensions_for_hints_and_exclusions(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] selection: &str,
    #[case] count: usize,
) {
    let yaml = format!(
        "- regex: '.*'\n  target: declaration\n  message: hi\n  {selection}\n\
         - regex: '.*'\n  target: declaration\n  action: exclude\n  {selection}"
    );
    let rules: Vec<SymbolRule> = serde_yml::from_str(&yaml).unwrap();
    let rules = compile_symbol_rules(&rules).unwrap();
    let parsed = backend_for(ext).unwrap().parse(source).unwrap();

    let result = check(&parsed, ext, &rules).unwrap();
    let exclusions = excluded_ranges(&parsed, ext, &rules).unwrap();

    assert_eq!(result.hints.len(), count);
    assert_eq!(result.excluded_ranges.len(), count);
    assert_eq!(exclusions, result.excluded_ranges);
}
