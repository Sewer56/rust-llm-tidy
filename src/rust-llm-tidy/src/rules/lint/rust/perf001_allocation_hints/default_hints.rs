//! Built-in Rust PERF001 reminders, kept apart from the matcher for
//! easy auditing.
//!
//! Scope: standard-library containers with a capacity-setting
//! alternative (`new` -> `with_capacity`). Containers without one
//! (`LinkedList`, `BTreeMap`, `BTreeSet`) stay out.
//!
//! Every message follows the shared diagnostic shape: finding, `Why:`,
//! `Suggestions:`.

use crate::config::PerfHint;
use std::borrow::Cow;

/// Built-in Rust reminders; a present `perf_hints` config replaces this
/// list.
pub(super) static DEFAULT_HINTS: &[PerfHint] = &[
    reminder(
        "Vec::new",
        concat!(
            "`Vec::new()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly reallocate as it grows.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use `Vec::with_capacity(count)`.\n",
            "- Keep `Vec::new()` when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "String::new",
        concat!(
            "`String::new()` starts with zero capacity.\n\n",
            "Why: appending text afterwards can repeatedly reallocate as it grows.\n\n",
            "Suggestions:\n",
            "- If the expected byte length is known, use `String::with_capacity(len)`.\n",
            "- Keep `String::new()` when the final length is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "VecDeque::new",
        concat!(
            "`VecDeque::new()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly reallocate as it grows.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use `VecDeque::with_capacity(count)`.\n",
            "- Keep `VecDeque::new()` when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "BinaryHeap::new",
        concat!(
            "`BinaryHeap::new()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly reallocate as it grows.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use `BinaryHeap::with_capacity(count)`.\n",
            "- Keep `BinaryHeap::new()` when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "HashMap::new",
        concat!(
            "`HashMap::new()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly rehash and reallocate as it grows.\n\n",
            "Suggestions:\n",
            "- If the expected entry count is known, use `HashMap::with_capacity(count)`.\n",
            "- Keep `HashMap::new()` when the final size is unknown; a wrong guess wastes memory."
        ),
    ),
    reminder(
        "HashSet::new",
        concat!(
            "`HashSet::new()` starts with zero capacity.\n\n",
            "Why: filling it afterwards can repeatedly rehash and reallocate as it grows.\n\n",
            "Suggestions:\n",
            "- If the expected element count is known, use `HashSet::with_capacity(count)`.\n",
            "- Keep `HashSet::new()` when the final size is unknown; a wrong guess wastes memory."
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
