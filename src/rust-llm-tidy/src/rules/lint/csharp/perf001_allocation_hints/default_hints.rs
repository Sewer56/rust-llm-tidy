//! Built-in C# PERF001 reminders, kept apart from the matcher for easy
//! auditing.
//!
//! Scope: standard collections with a capacity constructor. Linked,
//! tree-based, immutable, and concurrent collections stay out; their
//! construction semantics need separate consideration.
//!
//! Every message follows the shared diagnostic shape: finding, `Why:`,
//! `Suggestions:`.

use crate::config::PerfHint;
use std::borrow::Cow;

/// Built-in C# reminders; a present `perf_hints` config replaces this
/// list.
pub(super) static DEFAULT_HINTS: &[PerfHint] = &[
    reminder(
        "List::new",
        concat!(
            "`new List<T>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `List` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "Dictionary::new",
        concat!(
            "`new Dictionary<K, V>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize and rehash the underlying storage.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use the `Dictionary` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "HashSet::new",
        concat!(
            "`new HashSet<T>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize and rehash the underlying storage.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `HashSet` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "Queue::new",
        concat!(
            "`new Queue<T>()` starts without a chosen capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `Queue` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "Stack::new",
        concat!(
            "`new Stack<T>()` starts without a chosen capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `Stack` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "PriorityQueue::new",
        concat!(
            "`new PriorityQueue<TElement, TPriority>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying storage.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `PriorityQueue` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "SortedList::new",
        concat!(
            "`new SortedList<K, V>()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying arrays.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use the `SortedList` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "ArrayList::new",
        concat!(
            "`new ArrayList()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize the underlying array.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use the `ArrayList` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "Hashtable::new",
        concat!(
            "`new Hashtable()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly resize and rehash the underlying buckets.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use the `Hashtable` constructor that accepts a capacity.\n",
            "- Keep the parameterless constructor when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
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

/// One reminder entry with borrowed strings.
const fn reminder(pattern: &'static str, message: &'static str) -> PerfHint {
    PerfHint {
        pattern: Cow::Borrowed(pattern),
        message: Cow::Borrowed(message),
    }
}
