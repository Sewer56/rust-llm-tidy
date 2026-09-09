//! Built-in C# PERF001 reminders, kept apart from the matcher for easy
//! auditing.
//!
//! Scope: standard collections with a capacity constructor. Linked,
//! tree-based, immutable, and concurrent collections stay out; their
//! construction semantics need separate consideration.
//!
//! Every message follows the shared diagnostic shape: finding, `Why:`,
//! `Suggestions:`.

use crate::config::{CompiledSymbolRule, SymbolLanguage};

/// Constructor names and guidance, in first-match order.
const REMINDERS: &[(&str, &str)] = &[
    (
        "List::new",
        concat!(
            "`new List<T>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `List` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "Dictionary::new",
        concat!(
            "`new Dictionary<K, V>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize and rehash the underlying storage.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use the `Dictionary` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "HashSet::new",
        concat!(
            "`new HashSet<T>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize and rehash the underlying storage.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `HashSet` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "Queue::new",
        concat!(
            "`new Queue<T>()` starts without a chosen capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `Queue` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "Stack::new",
        concat!(
            "`new Stack<T>()` starts without a chosen capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `Stack` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "PriorityQueue::new",
        concat!(
            "`new PriorityQueue<TElement, TPriority>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying storage.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `PriorityQueue` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "SortedList::new",
        concat!(
            "`new SortedList<K, V>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying arrays.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use the `SortedList` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "ArrayList::new",
        concat!(
            "`new ArrayList()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `ArrayList` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "Hashtable::new",
        concat!(
            "`new Hashtable()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize and rehash the underlying buckets.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use the `Hashtable` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    (
        "StringBuilder::new",
        concat!(
            "`new StringBuilder()` starts without a chosen capacity.\n\n",
            "Why: appending text afterwards can repeatedly resize the internal buffer.\n\n",
            "Suggestions:\n",
            "- If the expected character count is known, use the `StringBuilder` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final length is unknown; a wrong guess wastes memory."
        ),
    ),
];

/// Built-in C# reminders enabled by the PERF001 family.
pub(super) fn reminders() -> Vec<CompiledSymbolRule> {
    REMINDERS
        .iter()
        .map(|(symbol, message)| super::capacity_reminder(SymbolLanguage::Csharp, symbol, message))
        .collect()
}
