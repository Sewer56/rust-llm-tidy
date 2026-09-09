//! Built-in symbol policies evaluated by the shared usage engine.

use crate::config::{
    ArrayKind, CompiledSymbolRule, SymbolAction, SymbolLanguage, SymbolMatcher, SymbolTarget,
};
use crate::reporting::Severity;
use crate::rules::registry::CODE_SYM;

#[cfg(test)]
mod capacity_tests;
mod csharp_capacity;
mod rust_capacity;

/// Remind only for explicit sized vectors, without inspecting surrounding loops.
pub(crate) fn array_reminder() -> CompiledSymbolRule {
    CompiledSymbolRule {
        extensions: [false, true],
        text_extensions: None,
        comments: Default::default(),
        array_kind: Some(ArrayKind::ExplicitSizedVector),
        target: SymbolTarget::Usage,
        matcher: SymbolMatcher::Literal("new[]".into()),
        action: SymbolAction::Hint,
        exclude_lints: 0,
        exclude_edits: false,
        exclude_post_process: false,
        title: Some("PERF002: array initialization reminder".into()),
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
        capacity_reminder: false,
        code: CODE_SYM,
    }
}

/// Compile the selected language's PERF001 capacity reminders.
pub(crate) fn capacity_reminders(language: SymbolLanguage) -> Vec<CompiledSymbolRule> {
    match language {
        SymbolLanguage::Rust => rust_capacity::reminders(),
        SymbolLanguage::Csharp => csharp_capacity::reminders(),
    }
}

/// Build a literal constructor policy without a separate matching implementation.
fn capacity_reminder(language: SymbolLanguage, symbol: &str, message: &str) -> CompiledSymbolRule {
    CompiledSymbolRule {
        text_extensions: None,
        comments: Default::default(),
        extensions: [
            language == SymbolLanguage::Rust,
            language == SymbolLanguage::Csharp,
        ],
        array_kind: None,
        target: SymbolTarget::Usage,
        matcher: SymbolMatcher::Literal(symbol.into()),
        action: SymbolAction::Hint,
        exclude_lints: 0,
        exclude_edits: false,
        exclude_post_process: false,
        title: Some("PERF001: API performance reminder".into()),
        message: Some(message.into()),
        scope: None,
        severity: Severity::Reminder,
        zero_arguments: Some(true),
        no_initializer: (language == SymbolLanguage::Csharp).then_some(true),
        capacity_reminder: true,
        code: CODE_SYM,
    }
}
