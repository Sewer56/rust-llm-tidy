//! PERF001 reminder entries: an invoked API pattern and its reminder text.

use serde::Deserialize;
use std::borrow::Cow;

/// One PERF001 reminder: an invoked API pattern and its reminder text.
///
/// The pattern names an API invocation and is matched structurally over
/// the parse, so mentions in comments and strings never fire:
///
/// - Rust: a call path (`Vec::new`), method name (`to_string`), or macro
///   name (`format!`). A pattern ending in `::new` (or `new`) matches
///   only zero-argument constructor calls; other patterns match calls
///   with any arguments.
/// - C#: a `Type::new` creation pattern (zero-argument constructions
///   only) or a final call name (`ToString`).
///
/// Matching is by written name only; same-named APIs on unrelated types
/// cannot be told apart without type resolution.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)] // Reject hallucinated hint keys at parse time.
pub struct PerfHint {
    /// Call path (`Vec::new`), method name (`to_string`), or macro name
    /// (`format!`) to match.
    ///
    /// In C#: a creation pattern (`List::new`) or a final call name
    /// (`ToString`).
    pub pattern: Cow<'static, str>,
    /// The reminder text emitted as the diagnostic message.
    pub message: Cow<'static, str>,
}
