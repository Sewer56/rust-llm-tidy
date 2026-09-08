//! Rust PERF001 defaults and the test-only legacy matcher reference.
//!
//! Every reminder entry names an invoked API; a match emits the entry's
//! message at reminder severity through the shared symbol engine.
//! Built-ins follow the shared `Why:`/`Suggestions:` diagnostic shape.
//!
//! The legacy reference retains hint severity for migration comparisons.
//!
//! Matching is syntax-only, so mentions in comments or strings never
//! fire. The reminder fires on the invocation alone; no loop or
//! hoisting is analyzed. The reminder just asks a human or model to
//! check the API choice.
//!
//! - Call-path patterns (`Vec::new`) match the callee by trailing
//!   components, so qualification (`std::vec::Vec::new`) and turbofish
//!   (`Vec::<u8>::new`) both match; method patterns (`to_string`)
//!   match the method name.
//! - Patterns ending in `new` are constructor patterns: they match only
//!   zero-argument calls, so `Thing::new(seed)` stays silent.
//! - Macro patterns (`format!`) match the bare macro name.
//!
//! `perf_hints` replaces the built-ins and `extra_perf_hints` applies
//! after them; the first matching entry wins.

use crate::config::PerfHint;
#[cfg(test)]
use crate::reporting::{Diagnostic, Severity};
#[cfg(test)]
use crate::rules::lint::CODE_PERF001;
#[cfg(test)]
use crate::source::ParseResult;
#[cfg(test)]
use tree_sitter::Node;

mod default_hints;

/// One tree walk matching every invocation against the reminder lists.
#[cfg(test)]
struct Walker<'a> {
    source: &'a str,
    /// Base list (the configured replacement or the built-ins).
    base: &'a [Reminder<'a>],
    /// Extra reminders, searched after `base`.
    extra: &'a [Reminder<'a>],
    diagnostics: Vec<Diagnostic>,
}

/// One reminder with its pattern pre-split once per [`check`] call.
#[cfg(test)]
struct Reminder<'a> {
    /// The raw pattern, compared for macro names.
    pattern: &'a str,
    /// The pattern's call-path components; empty patterns match nothing.
    components: Vec<&'a str>,
    /// The reminder text emitted as the diagnostic message.
    message: &'a str,
}

#[cfg(test)]
impl<'a> Walker<'a> {
    fn text(&self, node: Node<'_>) -> &'a str {
        self.source
            .get(node.byte_range())
            .unwrap_or_default()
            .trim()
    }

    fn line(&self, node: Node<'_>) -> usize {
        node.start_position().row + 1
    }

    /// Visit `node`, then its children, so nested invocations match too.
    fn visit(&mut self, node: Node) {
        match node.kind() {
            "call_expression" => self.check_call(node),
            "macro_invocation" => self.check_macro(node),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child);
        }
    }

    /// Match a call by its callee: method calls by method name, other
    /// callees by call-path components.
    fn check_call(&mut self, node: Node<'_>) {
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let callee = if function.kind() == "field_expression" {
            function
                .child_by_field_name("field")
                .map(|field| self.text(field))
                .unwrap_or_default()
        } else {
            self.text(function)
        };
        if callee.is_empty() {
            return;
        }
        let zero_args = node
            .child_by_field_name("arguments")
            .is_none_or(|args| args.named_child_count() == 0);
        if let Some(message) = self.find_match(callee, zero_args) {
            self.push_diagnostic(node, message, "call", callee);
        }
    }

    /// Match a macro invocation by its bare macro name.
    fn check_macro(&mut self, node: Node<'_>) {
        let Some(name) = node
            .child_by_field_name("macro")
            .map(|macro_name| self.text(macro_name))
        else {
            return;
        };
        if let Some(reminder) = self
            .base
            .iter()
            .chain(self.extra)
            .find(|reminder| matches_macro(reminder.pattern, name))
        {
            self.push_diagnostic(node, reminder.message, "macro", name);
        }
    }

    /// The first matching entry's message: `base` before `extra`.
    fn find_match(&self, callee: &str, zero_args: bool) -> Option<&'a str> {
        self.base
            .iter()
            .chain(self.extra)
            .find(|reminder| matches_call(reminder, callee, zero_args))
            .map(|reminder| reminder.message)
    }

    fn push_diagnostic(&mut self, node: Node<'_>, message: &str, kind: &str, name: &str) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Hint,
            code: CODE_PERF001,
            message: message.to_string(),
            line: self.line(node),
            item_kind: kind.to_string(),
            item_name: Some(name.to_string()),
        });
    }
}

/// Run every PERF001 reminder over the parsed Rust tree.
///
/// `hints` is the base list (the configured replacement or the
/// built-ins); `extra_hints` applies after it.
///
/// Matching is a single tree walk, linear in tree nodes. Patterns are
/// split once here; per invocation, matching borrows slices and
/// allocates nothing.
#[cfg(test)]
pub(crate) fn check(
    parsed: &ParseResult,
    hints: &[PerfHint],
    extra_hints: &[PerfHint],
) -> Vec<Diagnostic> {
    if hints.is_empty() && extra_hints.is_empty() {
        return Vec::new();
    }
    let base = reminders_for(hints);
    let extra = reminders_for(extra_hints);
    let mut walker = Walker {
        source: parsed.source.as_str(),
        base: &base,
        extra: &extra,
        diagnostics: Vec::new(),
    };
    walker.visit(parsed.syntax_tree().root_node());
    walker.diagnostics
}

/// The built-in Rust reminder list; a present `perf_hints` config
/// replaces it.
pub(crate) fn default_hints() -> &'static [PerfHint] {
    default_hints::DEFAULT_HINTS
}

/// True when `reminder` matches the invoked `callee`.
///
/// Constructor patterns (ending in `new`) match only zero-argument
/// calls, and patterns naming no components (only separators) match
/// nothing.
#[cfg(test)]
fn matches_call(reminder: &Reminder<'_>, callee: &str, zero_args: bool) -> bool {
    let is_constructor = reminder
        .components
        .last()
        .is_some_and(|last| *last == "new");
    (!is_constructor || zero_args) && callee_ends_with(callee, &reminder.components)
}

/// True when `pattern` names the macro `name`: macro patterns may keep
/// the trailing `!` (`format!`).
#[cfg(test)]
fn matches_macro(pattern: &str, name: &str) -> bool {
    pattern == name || pattern.strip_suffix('!') == Some(name)
}

/// Split every pattern once per [`check`] call.
#[cfg(test)]
fn reminders_for(hints: &[PerfHint]) -> Vec<Reminder<'_>> {
    hints
        .iter()
        .map(|hint| Reminder {
            pattern: hint.pattern.as_ref(),
            components: call_path(hint.pattern.as_ref()),
            message: hint.message.as_ref(),
        })
        .collect()
}

/// The call-path components of `text`: `std::vec::Vec::<u8>::new`
/// yields `["std", "vec", "Vec", "new"]` (turbofish segments removed).
#[cfg(test)]
fn call_path(text: &str) -> Vec<&str> {
    text.split("::")
        .map(str::trim)
        .filter(|part| !part.is_empty() && !part.starts_with('<'))
        .collect()
}

/// True when `callee` ends with `components`, ignoring turbofish
/// segments: `["Vec", "new"]` matches `std::vec::Vec::<u8>::new`.
///
/// Leading components are qualification, so a longer callee still
/// matches; an empty `components` never matches.
#[cfg(test)]
fn callee_ends_with(callee: &str, components: &[&str]) -> bool {
    let mut remaining = components.len();
    if remaining == 0 {
        return false;
    }
    for part in callee.rsplit("::").map(str::trim) {
        if remaining == 0 {
            return true;
        }
        if part.is_empty() || part.starts_with('<') {
            continue;
        }
        if part != components[remaining - 1] {
            return false;
        }
        remaining -= 1;
    }
    remaining == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;
    use crate::reporting::Severity;
    use rstest::rstest;

    /// One reminder entry for custom-list tests.
    fn hint(pattern: &str, message: &str) -> PerfHint {
        PerfHint {
            pattern: pattern.to_string().into(),
            message: message.to_string().into(),
        }
    }

    /// Parse `source` and run PERF001 with the built-in list.
    fn default_check(source: &str) -> Vec<Diagnostic> {
        check(&parse_source(source).unwrap(), default_hints(), &[])
    }

    /// The `(line, message)` pairs of all findings, for compact assertions.
    fn findings(source: &str) -> Vec<(usize, String)> {
        default_check(source)
            .iter()
            .map(|d| (d.line, d.message.clone()))
            .collect()
    }

    /// The built-in message for `pattern`, so tests assert the
    /// configured entry instead of restating its text.
    fn builtin(pattern: &str) -> String {
        default_hints()
            .iter()
            .find(|hint| hint.pattern.as_ref() == pattern)
            .unwrap_or_else(|| panic!("no built-in reminder for {pattern}"))
            .message
            .to_string()
    }

    /// Every built-in container constructor reminds on a standalone
    /// zero-argument call.
    #[rstest]
    #[case("Vec::new")]
    #[case("String::new")]
    #[case("VecDeque::new")]
    #[case("BinaryHeap::new")]
    #[case("HashMap::new")]
    #[case("HashSet::new")]
    fn builtins_should_remind_on_zero_argument_construction(#[case] call: &str) {
        let source = format!("fn f() {{\n    let v = {call}();\n}}\n");
        assert_eq!(
            findings(&source),
            vec![(2, builtin(call))],
            "the {call} call must carry its built-in reminder"
        );
    }

    /// Built-in messages follow the shared diagnostic shape: finding,
    /// `Why:`, `Suggestions:` with leading bullets.
    #[test]
    fn builtin_messages_should_carry_why_and_suggestions() {
        for hint in default_hints() {
            let message = hint.message.as_ref();
            let (_, rest) = message
                .split_once("\n\nWhy: ")
                .unwrap_or_else(|| panic!("{}: missing Why section", hint.pattern));
            let (_, suggestions) = rest
                .split_once("\n\nSuggestions:\n")
                .unwrap_or_else(|| panic!("{}: missing Suggestions section", hint.pattern));
            assert!(
                suggestions.starts_with("- "),
                "{}: suggestions must open with a bullet",
                hint.pattern
            );
        }
    }

    /// Qualified and turbofish constructor forms match the same
    /// built-in pattern.
    #[test]
    fn builtins_should_match_qualified_and_generic_construction() {
        let source =
            "fn f() {\n    let a = std::vec::Vec::new();\n    let b = Vec::<u8>::new();\n}\n";
        let expected = builtin("Vec::new");
        assert_eq!(
            findings(source),
            vec![(2, expected.clone()), (3, expected)],
            "qualification and turbofish must not hide the constructor"
        );
    }

    /// Argument-taking constructors, similarly named types, and
    /// non-invocation mentions stay silent.
    #[test]
    fn builtins_should_stay_silent_on_non_matching_or_mentioned_apis() {
        let source = concat!(
            "fn f() {\n",
            "    let a = Vec::with_capacity(4);\n",
            "    let b = OtherVec::new();\n",
            "    let c = \"Vec::new\";\n",
            "    // Vec::new()\n",
            "    let d = Vec::new;\n",
            "}\n",
        );
        assert!(
            default_check(source).is_empty(),
            "only zero-argument matching calls may fire:\n{source}"
        );
    }

    /// Patterns naming no components (only separators or turbofish
    /// segments) match nothing instead of every call.
    #[test]
    fn separator_only_patterns_should_match_nothing() {
        let parsed = parse_source("fn f() {\n    let v = Vec::new();\n}\n").unwrap();
        for pattern in ["::", "<u8>::"] {
            let extra = [hint(pattern, "catch-all")];
            assert!(
                check(&parsed, &[], &extra).is_empty(),
                "the {pattern:?} pattern must not match"
            );
        }
    }

    /// Patterns longer than the callee never match: a bare `new()` is
    /// not a `Vec::new`.
    #[test]
    fn patterns_should_stay_silent_when_longer_than_the_callee() {
        let source = "fn f() {\n    let v = new();\n}\n";
        assert!(
            default_check(source).is_empty(),
            "a short callee must not match a longer pattern:\n{source}"
        );
    }

    /// Constructor patterns apply to any `new`-shaped pattern; calls
    /// with arguments never match them.
    #[test]
    fn constructor_patterns_should_require_zero_arguments() {
        let extra = [hint("new", "check the new call")];
        let source = "fn f() {\n    let a = thing::new();\n    let b = thing::new(3);\n}\n";
        assert_eq!(
            check(&parse_source(source).unwrap(), &[], &extra)
                .iter()
                .map(|d| (d.line, d.message.clone()))
                .collect::<Vec<_>>(),
            vec![(2, "check the new call".to_string())],
            "the argument-taking new call must stay silent"
        );
    }

    /// Method and macro patterns match invocations of any shape.
    #[test]
    fn method_and_macro_patterns_should_match_invocations() {
        let extra = [
            hint("to_uppercase", "consider borrow or hoist"),
            hint("format!", "consider write instead"),
        ];
        let source = concat!(
            "fn f(prefix: &str, row: usize) {\n",
            "    let label = prefix.to_uppercase();\n",
            "    let text = format!(\"{prefix}{row}\");\n",
            "}\n",
        );
        assert_eq!(
            check(&parse_source(source).unwrap(), &[], &extra)
                .iter()
                .map(|d| (d.line, d.message.clone()))
                .collect::<Vec<_>>(),
            vec![
                (2, "consider borrow or hoist".to_string()),
                (3, "consider write instead".to_string()),
            ],
        );
    }

    /// Nested invocations each match on their own line.
    #[test]
    fn nested_invocations_should_each_remind() {
        let source =
            "fn f() {\n    let a = takes(Vec::new());\n    let b = takes(Vec::new());\n}\n";
        assert_eq!(findings(source).len(), 2, "each call matches once");
    }

    /// A base entry wins over a later extra with the same pattern, and
    /// extras still apply after an empty base.
    #[test]
    fn extras_should_apply_after_the_base_list() {
        let parsed = parse_source("fn f() {\n    let v = Vec::new();\n}\n").unwrap();
        let extra = [hint("Vec::new", "extra message")];

        let overlap = check(&parsed, default_hints(), &extra);
        assert_eq!(
            overlap
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>(),
            vec![builtin("Vec::new")],
            "the base entry wins without a duplicate finding"
        );

        let extras_only = check(&parsed, &[], &extra);
        assert_eq!(
            extras_only
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>(),
            vec!["extra message".to_string()],
            "an empty base leaves only the extras"
        );
    }

    /// Findings carry the PERF001 code at hint severity and name the
    /// invoked API.
    #[test]
    fn findings_should_be_hints_naming_the_invocation() {
        let findings = default_check("fn f() {\n    let v = std::vec::Vec::new();\n}\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, CODE_PERF001);
        assert_eq!(findings[0].severity, Severity::Hint);
        assert_eq!(findings[0].item_kind, "call");
        assert_eq!(findings[0].item_name.as_deref(), Some("std::vec::Vec::new"));
    }

    /// Empty base and extra lists disable every check.
    #[test]
    fn check_should_return_nothing_for_empty_lists() {
        let source = "fn f() {\n    let v = Vec::new();\n}\n";
        assert!(check(&parse_source(source).unwrap(), &[], &[]).is_empty());
    }
}
