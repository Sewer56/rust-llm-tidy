//! Declaration extraction acceptance tests using the pinned language grammars.

use super::declarations;
use crate::languages::backend_for;
use rstest::rstest;

#[rstest]
#[case::rust_method(
    "struct Cache;\nimpl Cache {\n /// Keep.\n #[inline]\n fn get(&self) { let x = 1; }\n}",
    "rs",
    "Cache::get",
    " /// Keep.\n #[inline]\n fn get(&self) { let x = 1; }",
    5
)]
#[case::rust_field(
    "struct Cache {\n /// Keep.\n #[allow(dead_code)]\n value: usize,\n}",
    "rs",
    "Cache::value",
    " /// Keep.\n #[allow(dead_code)]\n value: usize",
    4
)]
#[case::rust_impl(
    "struct Cache<T>(T);\n/// Keep impl.\nimpl<T> Cache<T> { fn get(&self) {} }",
    "rs",
    "Cache",
    "/// Keep impl.\nimpl<T> Cache<T> { fn get(&self) {} }",
    3
)]
#[case::rust_module(
    "mod storage { struct Cache; impl Cache { fn get() {} } }",
    "rs",
    "storage::Cache::get",
    "fn get() {}",
    1
)]
#[case::csharp_method(
    "class Cache {\n /// Keep.\n [Obsolete]\n void Get() { var x = 1; }\n}",
    "cs",
    "Cache::Get",
    " /// Keep.\n [Obsolete]\n void Get() { var x = 1; }",
    4
)]
#[case::csharp_field(
    "class Cache {\n /// Keep.\n [Obsolete] int first, second;\n}",
    "cs",
    "Cache::second",
    " /// Keep.\n [Obsolete] int first, second;",
    3
)]
#[case::csharp_property(
    "class Cache {\n /// Keep.\n int Value { get; set; }\n}",
    "cs",
    "Cache::Value",
    " /// Keep.\n int Value { get; set; }",
    3
)]
#[case::csharp_namespace(
    "namespace A.B;\nclass Cache { void Get() {} }",
    "cs",
    "A::B::Cache::Get",
    "void Get() {}",
    2
)]
fn declarations_should_include_owned_source(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] path: &str,
    #[case] expected: &str,
    #[case] name_line: usize,
) {
    let parsed = backend_for(ext).unwrap().parse(source).unwrap();

    let declarations = declarations(&parsed, ext).unwrap();
    let declaration = declarations
        .iter()
        .find(|declaration| {
            declaration.path.as_ref() == path && &source[declaration.bytes.clone()] == expected
        })
        .expect("declaration includes owned bytes");

    assert_eq!(declaration.name_lines, name_line..=name_line);
}

#[rstest]
#[case::rust("struct Cache {", "rs")]
#[case::csharp("class Cache {", "cs")]
fn declarations_should_reject_syntax_errors(#[case] source: &str, #[case] ext: &str) {
    let parsed = backend_for(ext).unwrap().parse(source).unwrap();

    let result = declarations(&parsed, ext);

    assert!(result.is_err());
}

#[test]
fn declarations_should_share_all_field_bytes() {
    let source = "class Cache { [Obsolete] int first, second; }";
    let parsed = backend_for("cs").unwrap().parse(source).unwrap();

    let declarations = declarations(&parsed, "cs").unwrap();
    let first = declarations
        .iter()
        .find(|item| item.path.as_ref() == "Cache::first")
        .unwrap();
    let second = declarations
        .iter()
        .find(|item| item.path.as_ref() == "Cache::second")
        .unwrap();

    assert_eq!(first.bytes, second.bytes);
    assert_eq!(
        &source[first.bytes.clone()],
        "[Obsolete] int first, second;"
    );
}
