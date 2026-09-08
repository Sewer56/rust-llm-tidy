//! Test written names, independent exclusions, and legacy output.

use super::*;
use crate::config::PerfHint;
use crate::config::{SymbolRule, compile_symbol_rules};
use crate::languages::{csharp, rust};
use crate::reporting::Severity;
use crate::rules::lint::{CODE_PERF001, CODE_SYM001, csharp as csharp_rules, rust as rust_rules};
use rstest::rstest;

#[rstest]
#[case::rust("mod generated { fn run() {} }", "rs", "generated::run")]
#[case::csharp(
    "namespace Generated { class Worker { void Run() {} } }",
    "cs",
    "Generated::Worker::Run"
)]
fn declaration_hints_should_match_qualified_paths(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] symbol: &str,
) {
    let yaml = format!("- symbol: {symbol}\n  target: declaration\n  message: reminder");

    let result = observe(source, ext, &yaml);

    assert_eq!(result.hints.len(), 1, "{result:?}");
    assert_eq!(
        result.hints[0].diagnostic.item_name.as_deref(),
        Some(symbol)
    );
}

#[rstest]
#[case::exclude_first(true)]
#[case::exclude_last(false)]
fn exclusions_should_apply_independently_of_hint_order(#[case] exclude_first: bool) {
    let source = "/// owned docs\n#[inline]\nfn generated() { run(); }\nfn retained() {}";
    let hint = "- symbol: generated\n  target: declaration\n  message: reminder\n";
    let exclude = "- symbol: generated\n  target: declaration\n  action: exclude\n";
    let yaml = if exclude_first {
        format!("{exclude}{hint}")
    } else {
        format!("{hint}{exclude}")
    };

    let result = observe(source, "rs", &yaml);

    assert_eq!(result.hints.len(), 1);
    assert_eq!(result.hints[0].diagnostic.line, 3);
    assert_eq!(result.excluded_ranges.len(), 1);
    assert_eq!(
        &source[result.excluded_ranges[0].clone()],
        "/// owned docs\n#[inline]\nfn generated() { run(); }"
    );
}

#[rstest]
#[case::rust("fn broken( {", "rs")]
#[case::csharp("class Broken { void Run( }", "cs")]
fn exclusions_should_fail_closed_on_syntax_errors(#[case] source: &str, #[case] ext: &str) {
    let rules: Vec<SymbolRule> =
        serde_yml::from_str("- regex: '.*'\n  target: declaration\n  action: exclude").unwrap();
    let rules = compile_symbol_rules(&rules).unwrap();
    let parsed = if ext == "rs" {
        rust::parse::parse_source(source).unwrap()
    } else {
        csharp::parse::parse(source).unwrap()
    };

    let result = check(&parsed, ext, &rules);

    assert!(result.is_err());
}

#[test]
fn exclusions_should_return_ranges_without_hints() {
    let source = "fn generated() {}\nfn retained() {}";
    let yaml = "- symbol: generated\n  target: declaration\n  action: exclude";

    let result = observe(source, "rs", yaml);

    assert!(result.hints.is_empty());
    assert_eq!(result.excluded_ranges, vec![0..source.find('\n').unwrap()]);
}

#[rstest]
#[case::rust_zero("fn f() { Vec::new(); }", "rs", true, 1)]
#[case::rust_arguments("fn f() { Vec::new(8); }", "rs", true, 0)]
#[case::rust_comment_argument("fn f() { Vec::new(/* empty */); }", "rs", true, 1)]
#[case::rust_require_arguments("fn f() { Vec::new(8); }", "rs", false, 1)]
#[case::csharp_zero("class C { void F() { new Vec(); } }", "cs", true, 1)]
#[case::csharp_arguments("class C { void F() { new Vec(8); } }", "cs", true, 0)]
fn hints_should_apply_explicit_argument_conditions(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] zero: bool,
    #[case] count: usize,
) {
    let yaml = format!("- symbol: Vec::new\n  message: reminder\n  zero_arguments: {zero}");

    let result = observe(source, ext, &yaml);

    assert_eq!(result.hints.len(), count);
}

#[rstest]
#[case::without_initializer("new Vec()", true, 1)]
#[case::with_initializer("new Vec() { 1 }", true, 0)]
#[case::require_initializer("new Vec { 1 }", false, 1)]
fn hints_should_apply_explicit_initializer_conditions(
    #[case] creation: &str,
    #[case] no_initializer: bool,
    #[case] count: usize,
) {
    let source = format!("class C {{ void F() {{ {creation}; }} }}");
    let yaml =
        format!("- symbol: Vec::new\n  message: reminder\n  no_initializer: {no_initializer}");

    let result = observe(&source, "cs", &yaml);

    assert_eq!(result.hints.len(), count);
}

#[rstest]
#[case::rust_comment_and_string(
    "fn f() { /* Vec::new() */ let x = \"Vec::new()\"; }",
    "rs",
    "Vec::new"
)]
#[case::csharp_comment_and_string(
    "class C { void F() { /* new List() */ var s = \"new List()\"; } }",
    "cs",
    "List::new"
)]
#[case::rust_receiver_type("fn f() { value.to_string(); }", "rs", "String::to_string")]
#[case::rust_import_alias("use std::vec::Vec as V; fn f() { V::new(); }", "rs", "Vec::new")]
#[case::csharp_receiver_type(
    "class C { void F() { value.ToString(); } }",
    "cs",
    "String::ToString"
)]
#[case::csharp_import_namespace(
    "using System; class C { void F() { String.Join(); } }",
    "cs",
    "System::String::Join"
)]
#[case::csharp_target_typed_creation("class C { List<int> x = new(); }", "cs", "List::new")]
fn hints_should_ignore_text_and_unresolved_names(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] symbol: &str,
) {
    let yaml = format!("- symbol: {symbol}\n  message: reminder");

    let result = observe(source, ext, &yaml);

    assert!(result.hints.is_empty(), "{result:?}");
}

#[test]
fn hints_should_keep_first_match_and_its_scope() {
    let yaml =
        "- symbol: new\n  message: first\n  scope: all\n- symbol: Vec::new\n  message: second";

    let result = observe("fn f() { Vec::new(); }", "rs", yaml);

    assert_eq!(result.hints.len(), 1);
    let hint = &result.hints[0];
    assert_eq!(hint.diagnostic.code, CODE_SYM001);
    assert_eq!(hint.diagnostic.severity, Severity::Reminder);
    assert_eq!(hint.diagnostic.message, "first");
    assert_eq!(hint.scope, Some(ReportingScope::All));
}

#[rstest]
#[case::rust_suffix("fn f() { std::vec::Vec::<u8>::new(); }", "rs", "symbol: Vec::new", 1)]
#[case::rust_component_boundary("fn f() { OtherVec::new(); }", "rs", "symbol: Vec::new", 0)]
#[case::regex_whole_name("fn f() { std::vec::Vec::<u8>::new(); }", "rs", "regex: 'Vec::new'", 0)]
#[case::regex_explicit_prefix(
    "fn f() { std::vec::Vec::<u8>::new(); }",
    "rs",
    "regex: '.*::Vec::new'",
    1
)]
#[case::regex_alternation("fn f() { OtherVec::new(); }", "rs", "regex: 'Vec::new|Other'", 0)]
#[case::rust_method("fn f() { value.to_string(); }", "rs", "symbol: to_string", 1)]
#[case::rust_generic_method("fn f() { value.collect::<Vec<u8>>(); }", "rs", "symbol: collect", 1)]
#[case::rust_macro("fn f() { std::format!(\"x\"); }", "rs", "symbol: format!", 1)]
#[case::csharp_creation(
    "class C { void F() { new System.Collections.Generic.List<int>(); } }",
    "cs",
    "symbol: List::new",
    1
)]
#[case::csharp_invocation(
    "class C { void F() { System.String.Join(); } }",
    "cs",
    "symbol: String::Join",
    1
)]
#[case::csharp_generic_invocation(
    "class C { void F() { values.Select<int>(); } }",
    "cs",
    "symbol: Select",
    1
)]
#[case::constructor_with_arguments("fn f() { Vec::new(1); }", "rs", "symbol: Vec::new", 1)]
fn hints_should_match_written_component_names(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] matcher: &str,
    #[case] count: usize,
) {
    let yaml = format!("- {matcher}\n  message: reminder");

    let result = observe(source, ext, &yaml);

    assert_eq!(result.hints.len(), count, "{result:?}");
}

#[rstest]
#[case::rust("fn f() {\n  Vec\n    ::new(\n    );\n}", "rs", "Vec::new", 3)]
#[case::rust_method("fn f() {\n  value\n    .to_string(\n    );\n}", "rs", "to_string", 3)]
#[case::csharp(
    "class C { void F() {\n  System.String\n    .Join(\n    );\n} }",
    "cs",
    "String::Join",
    3
)]
fn hints_should_report_on_api_name_line(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] symbol: &str,
    #[case] line: usize,
) {
    let yaml = format!("- symbol: {symbol}\n  message: reminder");

    let result = observe(source, ext, &yaml);

    assert_eq!(result.hints.len(), 1);
    assert_eq!(result.hints[0].diagnostic.line, line);
    assert_eq!(result.hints[0].scope, None);
}

#[rstest]
#[case::rust_in_rust("fn f() { run(); }", "rs", "rust", 1)]
#[case::csharp_in_rust("fn f() { run(); }", "rs", "csharp", 0)]
#[case::rust_in_csharp("class C { void F() { run(); } }", "cs", "rust", 0)]
#[case::csharp_in_csharp("class C { void F() { run(); } }", "cs", "csharp", 1)]
fn hints_should_respect_language_selection(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] language: &str,
    #[case] count: usize,
) {
    let yaml = format!("- symbol: run\n  message: reminder\n  language: {language}");

    let result = observe(source, ext, &yaml);

    assert_eq!(result.hints.len(), count);
}

#[rstest]
#[case::base_first(false, "base")]
#[case::empty_base(true, "extra")]
fn legacy_adapter_should_preserve_base_before_extra_order(
    #[case] empty_base: bool,
    #[case] expected: &str,
) {
    let parsed = rust::parse::parse_source("fn f() { run(); }").unwrap();
    let base = [PerfHint {
        pattern: "run".into(),
        message: "base".into(),
    }];
    let extra = [PerfHint {
        pattern: "run".into(),
        message: "extra".into(),
    }];
    let base = if empty_base { &[][..] } else { &base[..] };
    let rules = legacy::compile_legacy_hints(SymbolLanguage::Rust, base, &extra);

    let mut old = rust_rules::perf001_allocation_hints::check(&parsed, base, &extra);
    old[0].severity = Severity::Reminder;
    let new = check(&parsed, "rs", &rules).unwrap();

    assert_eq!(new.hints.len(), 1);
    assert_eq!(new.hints[0].diagnostic.message, expected);
    assert_eq!(new.hints[0].diagnostic.to_string(), old[0].to_string());
}

#[rstest]
#[case::rust(
    "fn f() { std::vec::Vec::<u8>::new(); Vec::new(1); value.to_string(); format!(\"x\"); }",
    "rs"
)]
#[case::csharp(
    "class C { void F() { new System.Collections.Generic.List<int>(); new List<int>(8); new List<int> { 1 }; value.ToString(); } }",
    "cs"
)]
fn legacy_adapter_should_preserve_rendered_findings(#[case] source: &str, #[case] ext: &str) {
    let (parsed, hints, mut old) = if ext == "rs" {
        let parsed = rust::parse::parse_source(source).unwrap();
        let hints = rust_rules::perf001_allocation_hints::default_hints();
        let old = rust_rules::perf001_allocation_hints::check(&parsed, hints, &[]);
        (parsed, hints, old)
    } else {
        let parsed = csharp::parse::parse(source).unwrap();
        let hints = csharp_rules::perf001_allocation_hints::default_hints();
        let old = csharp_rules::perf001_allocation_hints::check(&parsed, hints, &[]);
        (parsed, hints, old)
    };
    let rules =
        legacy::compile_legacy_hints(SymbolLanguage::for_extension(ext).unwrap(), hints, &[]);
    for diagnostic in &mut old {
        diagnostic.severity = Severity::Reminder;
    }

    let result = check(&parsed, ext, &rules).unwrap();
    let new: Vec<_> = result
        .hints
        .into_iter()
        .map(|hint| hint.diagnostic)
        .collect();

    assert!(!old.is_empty());
    assert_eq!(new, old);
    assert_eq!(
        new.iter().map(ToString::to_string).collect::<Vec<_>>(),
        old.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
    assert!(new.iter().all(|diagnostic| diagnostic.code == CODE_PERF001));
}

/// Parse a hermetic source fixture and apply YAML policies through the engine.
fn observe(source: &str, ext: &str, yaml: &str) -> SymbolObservations {
    let rules: Vec<SymbolRule> = serde_yml::from_str(yaml).unwrap();
    let rules = compile_symbol_rules(&rules).unwrap();
    let parsed = match ext {
        "rs" => rust::parse::parse_source(source).unwrap(),
        "cs" => csharp::parse::parse(source).unwrap(),
        _ => unreachable!(),
    };

    check(&parsed, ext, &rules).unwrap()
}
