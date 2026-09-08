//! Built-in symbol policies evaluated by the shared usage engine.

use crate::config::{ArrayKind, CompiledSymbolRule, SymbolAction, SymbolMatcher, SymbolTarget};
use crate::reporting::Severity;
use crate::rules::registry::CODE_PERF002;

/// Remind only for explicit sized vectors, without inspecting surrounding loops.
pub(crate) fn array_reminder() -> CompiledSymbolRule {
    CompiledSymbolRule {
        language: None,
        extensions: [false, true],
        array_kind: Some(ArrayKind::ExplicitSizedVector),
        target: SymbolTarget::Usage,
        matcher: SymbolMatcher::Literal("new[]".into()),
        action: SymbolAction::Hint,
        message: Some(concat!(
            "Regular array allocation zero-initializes elements.\n",
            "Why: Zero initialization may be redundant if every element is overwritten.\n",
            "Suggestions:\n",
            "- Consider GC.AllocateUninitializedArray<T>(length) only when the runtime supports it ",
            "and every element is initialized before any read.\n",
            "- Keep regular allocation when zero values are needed. The API may still zero arrays ",
            "containing references; no performance gain is guaranteed.\n",
            "- Arrays with initializers are not a straightforward replacement."
        ).into()),
        scope: None,
        severity: Severity::Reminder,
        zero_arguments: None,
        no_initializer: Some(true),
        code: CODE_PERF002,
    }
}
