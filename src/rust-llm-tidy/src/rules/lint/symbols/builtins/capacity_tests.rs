//! Capacity reminder acceptance through the shared symbol engine.

use super::*;
use crate::languages::backend_for;
use crate::rules::lint::symbols;
use rstest::rstest;

// Syntax-only exclusions, constructor constraints, and language boundaries.
#[rstest]
#[case::rust_capacity("Vec::with_capacity(4);", "rs")]
#[case::rust_arguments("Vec::new(4);", "rs")]
#[case::rust_comment_arguments("Vec::new(/* empty */);", "rs")]
#[case::rust_other_type("OtherVec::new();", "rs")]
#[case::rust_short_name("new();", "rs")]
#[case::rust_mention("let a = \"Vec::new()\"; /* Vec::new() */ let b = Vec::new;", "rs")]
#[case::rust_csharp_type("List::new();", "rs")]
#[case::csharp_capacity("new List<int>(10);", "cs")]
#[case::csharp_copy("new List<int>(items);", "cs")]
#[case::csharp_comment_arguments("new List<int>(/* empty */);", "cs")]
#[case::csharp_initializer("new List<int> { 1 };", "cs")]
#[case::csharp_empty_initializer("new List<int>() {};", "cs")]
#[case::csharp_other_type("new LinkedList<int>();", "cs")]
#[case::csharp_mention("var a = \"new List<int>()\"; /* new List<int>() */", "cs")]
#[case::csharp_target_typed("List<int> a = new();", "cs")]
#[case::csharp_collection("List<int> a = [];", "cs")]
#[case::csharp_rust_type("new Vec();", "cs")]
fn reminders_should_ignore_nonmatching_syntax(#[case] body: &str, #[case] ext: &str) {
    let source = fixture(body, ext);
    let parsed = backend_for(ext).unwrap().parse(&source).unwrap();
    let mut rules = capacity_reminders(SymbolLanguage::Rust);
    rules.extend(capacity_reminders(SymbolLanguage::Csharp));

    let result = symbols::check(&parsed, ext, &rules).unwrap();

    assert!(result.hints.is_empty(), "{result:?}");
}

// Every built-in and its complete diagnostic metadata.
#[rstest]
#[case::vec("Vec::new", "rs", "Vec::new", "Vec::new")]
#[case::string("String::new", "rs", "String::new", "String::new")]
#[case::deque("VecDeque::new", "rs", "VecDeque::new", "VecDeque::new")]
#[case::heap("BinaryHeap::new", "rs", "BinaryHeap::new", "BinaryHeap::new")]
#[case::map("HashMap::new", "rs", "HashMap::new", "HashMap::new")]
#[case::set("HashSet::new", "rs", "HashSet::new", "HashSet::new")]
#[case::qualified("std::vec::Vec::new", "rs", "Vec::new", "std::vec::Vec::new")]
#[case::generic("Vec::<u8>::new", "rs", "Vec::new", "Vec::<u8>::new")]
#[case::multiline("Vec\n::new", "rs", "Vec::new", "Vec\n::new")]
#[case::list("new List<int>", "cs", "List::new", "List")]
#[case::dictionary(
    "new System.Collections.Generic.Dictionary<string, int>",
    "cs",
    "Dictionary::new",
    "Dictionary"
)]
#[case::hash_set("new HashSet<int>", "cs", "HashSet::new", "HashSet")]
#[case::queue("new Queue<int>", "cs", "Queue::new", "Queue")]
#[case::stack("new Stack<string>", "cs", "Stack::new", "Stack")]
#[case::priority(
    "new PriorityQueue<int, int>",
    "cs",
    "PriorityQueue::new",
    "PriorityQueue"
)]
#[case::sorted("new SortedList<string, int>", "cs", "SortedList::new", "SortedList")]
#[case::array_list(
    "new System.Collections.ArrayList",
    "cs",
    "ArrayList::new",
    "ArrayList"
)]
#[case::hashtable(
    "new System.Collections.Hashtable",
    "cs",
    "Hashtable::new",
    "Hashtable"
)]
#[case::builder(
    "new System.Text.StringBuilder",
    "cs",
    "StringBuilder::new",
    "StringBuilder"
)]
#[case::multiline_creation("new\n List<int>", "cs", "List::new", "List")]
fn reminders_should_report_constructor_guidance(
    #[case] callee: &str,
    #[case] ext: &str,
    #[case] symbol: &str,
    #[case] name: &str,
) {
    let source = fixture(&format!("Take({callee}());"), ext);
    let parsed = backend_for(ext).unwrap().parse(&source).unwrap();
    let rules = capacity_reminders(SymbolLanguage::for_extension(ext).unwrap());
    let rule = rules.iter().find(|rule| {
        matches!(&rule.matcher, SymbolMatcher::Literal(value) if value.as_ref() == symbol)
    }).unwrap();

    let result = symbols::check(&parsed, ext, &rules).unwrap();

    assert_eq!(result.hints.len(), 1);
    let diagnostic = &result.hints[0].diagnostic;
    assert_eq!(diagnostic.message, rule.message.as_deref().unwrap());
    assert_eq!(
        diagnostic.title.as_deref(),
        Some("PERF001: API performance reminder")
    );
    assert_eq!(diagnostic.code, CODE_SYM);
    assert_eq!(diagnostic.severity, Severity::Reminder);
    assert_eq!(diagnostic.line, 2 + callee.matches('\n').count());
    assert_eq!(diagnostic.item_name.as_deref(), Some(name));
    assert_eq!(
        diagnostic.item_kind,
        if ext == "rs" { "call" } else { "creation" }
    );
    assert!(diagnostic.message.contains("\n\nWhy: "));
    assert!(diagnostic.message.contains("\n\nSuggestions:\n- "));
}

#[rstest]
#[case::rust("Take(Vec::new());\nTake(Vec::new());", "rs")]
#[case::csharp("Take(new List<int>());\nTake(new List<int>());", "cs")]
fn reminders_should_report_each_nested_constructor(#[case] body: &str, #[case] ext: &str) {
    let source = fixture(body, ext);
    let parsed = backend_for(ext).unwrap().parse(&source).unwrap();
    let rules = capacity_reminders(SymbolLanguage::for_extension(ext).unwrap());

    let result = symbols::check(&parsed, ext, &rules).unwrap();

    assert_eq!(
        result
            .hints
            .iter()
            .map(|hint| hint.diagnostic.line)
            .collect::<Vec<_>>(),
        [2, 3]
    );
}

/// Place invocation syntax on the second line of a language-specific body.
fn fixture(body: &str, ext: &str) -> String {
    if ext == "rs" {
        format!("fn f() {{\n{body}\n}}")
    } else {
        format!("class C {{ void M() {{\n{body}\n}} }}")
    }
}
