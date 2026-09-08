//! C# PERF001 defaults and the test-only legacy matcher reference.
//!
//! Every reminder entry names an invoked API; a match emits the entry's
//! message at reminder severity through the shared symbol engine.
//! Built-ins follow the shared `Why:`/`Suggestions:` diagnostic shape.
//!
//! The legacy reference retains hint severity for migration comparisons.
//!
//! Matching is syntax-only, so mentions in comments or strings never
//! fire. The reminder fires on the invocation alone; no fill loop or
//! hoisting is analyzed. The reminder just asks a human or model to
//! check the API choice.
//!
//! - Creation patterns (`List::new`) match `new T()` by the type's
//!   unqualified, ungenericized base name.
//! - Creations match only with zero arguments and no initializer, so
//!   `new List<int>(capacity)` and `new List<int> { 1, 2 }` stay
//!   silent.
//! - Call patterns (`ToString`) match an invocation's final name:
//!   member calls by member name, plain calls by callee text.
//!
//! Only explicit `new T(...)` creations match; target-typed `new()`
//! and collection expressions are out of scope.
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
    base: &'a [PerfHint],
    /// Extra reminders, searched after `base`.
    extra: &'a [PerfHint],
    diagnostics: Vec<Diagnostic>,
}

#[cfg(test)]
impl<'a> Walker<'a> {
    fn text(&self, node: Node<'_>) -> &'a str {
        // `source` borrows the parsed text, not `self`, so slices
        // outlive later mutable walker calls.
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
            "object_creation_expression" => self.check_creation(node),
            "invocation_expression" => self.check_invocation(node),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child);
        }
    }

    /// Match a `new T()` creation by the created type's base name, but
    /// only for zero-argument constructions without an initializer.
    fn check_creation(&mut self, node: Node<'_>) {
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };
        let bare = bare_type_name(self.text(type_node));
        let zero_arg_constructor = node
            .child_by_field_name("arguments")
            .is_some_and(|args| args.named_child_count() == 0)
            && node.child_by_field_name("initializer").is_none();
        if !zero_arg_constructor {
            return;
        }
        if let Some(message) = self
            .base
            .iter()
            .chain(self.extra)
            .find(|hint| hint.pattern.strip_suffix("::new") == Some(bare))
            .map(|hint| hint.message.as_ref())
        {
            self.push_diagnostic(node, message, "creation", bare);
        }
    }

    /// Match an invocation by its final name: member calls
    /// (`x.ToString()`) by member name, plain calls by callee text.
    fn check_invocation(&mut self, node: Node<'_>) {
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let name = if function.kind() == "member_access_expression" {
            function
                .child_by_field_name("name")
                .map(|name| self.text(name))
                .unwrap_or_default()
        } else {
            self.text(function)
        };
        if name.is_empty() {
            return;
        }
        if let Some(message) = self
            .base
            .iter()
            .chain(self.extra)
            .find(|hint| hint.pattern.as_ref() == name)
            .map(|hint| hint.message.as_ref())
        {
            self.push_diagnostic(node, message, "call", name);
        }
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

/// Run every PERF001 reminder over the parsed C# tree.
///
/// `hints` is the base list (the configured replacement or the
/// built-ins); `extra_hints` applies after it.
///
/// Matching is a single tree walk, linear in tree nodes, with no heap
/// use beyond the returned diagnostics.
#[cfg(test)]
pub(crate) fn check(
    parsed: &ParseResult,
    hints: &[PerfHint],
    extra_hints: &[PerfHint],
) -> Vec<Diagnostic> {
    if hints.is_empty() && extra_hints.is_empty() {
        return Vec::new();
    }
    let mut walker = Walker {
        source: parsed.source.as_str(),
        base: hints,
        extra: extra_hints,
        diagnostics: Vec::new(),
    };
    walker.visit(parsed.syntax_tree().root_node());
    walker.diagnostics
}

/// The built-in C# reminder list; a present `perf_hints` config
/// replaces it.
pub(crate) fn default_hints() -> &'static [PerfHint] {
    default_hints::DEFAULT_HINTS
}

/// The matchable base name of a created type: generic arguments and
/// namespace qualification come off, so
/// `System.Collections.Generic.List<T>` yields `List`.
#[cfg(test)]
fn bare_type_name(type_text: &str) -> &str {
    let without_generics = type_text.split('<').next().unwrap_or(type_text);
    without_generics
        .rsplit('.')
        .next()
        .unwrap_or(without_generics)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;
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
        check(&parse(source).unwrap(), default_hints(), &[])
    }

    /// The `(line, message)` pairs of all findings, for compact assertions.
    fn findings(source: &str) -> Vec<(usize, String)> {
        default_check(source)
            .iter()
            .map(|d| (d.line, d.message.clone()))
            .collect()
    }

    /// The built-in message for the `pattern` behind a `created` type
    /// text, so tests assert the configured entry instead of restating
    /// its text.
    fn builtin(created: &str) -> String {
        let pattern = format!("{}::new", bare_type_name(created));
        default_hints()
            .iter()
            .find(|hint| hint.pattern.as_ref() == pattern)
            .unwrap_or_else(|| panic!("no built-in reminder for {pattern}"))
            .message
            .to_string()
    }

    /// Every built-in container constructor reminds on a standalone
    /// zero-argument creation.
    #[rstest]
    #[case("List<int>")]
    #[case("System.Collections.Generic.Dictionary<string, int>")]
    #[case("HashSet<int>")]
    #[case("Queue<int>")]
    #[case("Stack<string>")]
    #[case("PriorityQueue<int, int>")]
    #[case("SortedList<string, int>")]
    #[case("System.Collections.ArrayList")]
    #[case("System.Collections.Hashtable")]
    #[case("System.Text.StringBuilder")]
    fn builtins_should_remind_on_zero_argument_creation(#[case] created: &str) {
        let source = format!("class C {{\n    void M() {{ var v = new {created}(); }}\n}}\n");
        assert_eq!(
            findings(&source),
            vec![(2, builtin(created))],
            "the {created} creation must carry its built-in reminder"
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

    /// Constructions that pass a capacity, copy a collection, or carry
    /// an initializer stay silent.
    #[test]
    fn builtins_should_stay_silent_on_shaped_or_initialized_construction() {
        let source = concat!(
            "class C {\n",
            "    void M(int[] items) {\n",
            "        var a = new List<int>(10);\n",
            "        var b = new List<int>(items);\n",
            "        var c = new List<int> { 1, 2, 3 };\n",
            "        var d = new System.Text.StringBuilder(64);\n",
            "    }\n",
            "}\n",
        );
        assert!(
            default_check(source).is_empty(),
            "shaped constructions already choose an overload:\n{source}"
        );
    }

    /// Mentions in comments and strings, and unrelated creations, stay
    /// silent.
    #[test]
    fn builtins_should_stay_silent_on_mentions_and_unrelated_types() {
        let source = concat!(
            "class C {\n",
            "    void M() {\n",
            "        var a = \"new List<int>()\";\n",
            "        // new List<int>()\n",
            "        var b = new LinkedList<int>();\n",
            "    }\n",
            "}\n",
        );
        assert!(
            default_check(source).is_empty(),
            "only matching zero-argument creations may fire:\n{source}"
        );
    }

    /// Call patterns match member and plain invocations of any shape.
    #[test]
    fn call_patterns_should_match_member_and_plain_invocations() {
        let extra = [
            hint("ToString", "consider invariant culture"),
            hint("Render", "plain reminder"),
        ];
        let source = concat!(
            "class C {\n",
            "    void M(string prefix, int row) {\n",
            "        Render();\n",
            "        var a = prefix.ToString();\n",
            "        var b = prefix.ToString(\"X\");\n",
            "    }\n",
            "}\n",
        );
        assert_eq!(
            check(&parse(source).unwrap(), &[], &extra)
                .iter()
                .map(|d| (d.line, d.message.clone()))
                .collect::<Vec<_>>(),
            vec![
                (3, "plain reminder".to_string()),
                (4, "consider invariant culture".to_string()),
                (5, "consider invariant culture".to_string()),
            ],
        );
    }

    /// A creation nested inside an enclosing call still matches.
    #[test]
    fn nested_creation_should_remind_inside_an_enclosing_call() {
        let source = "class C {\n    void M() { Take(new List<int>()); }\n}\n";
        assert_eq!(
            findings(source),
            vec![(2, builtin("List<int>"))],
            "the nested creation must carry its built-in reminder"
        );
    }

    /// A base entry wins over a later extra with the same pattern, and
    /// extras still apply after an empty base.
    #[test]
    fn extras_should_apply_after_the_base_list() {
        let parsed = parse("class C {\n    void M() { var v = new List<int>(); }\n}\n").unwrap();
        let extra = [hint("List::new", "extra message")];

        let overlap = check(&parsed, default_hints(), &extra);
        assert_eq!(
            overlap
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>(),
            vec![builtin("List<int>")],
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
    /// created type.
    #[test]
    fn findings_should_be_hints_naming_the_creation() {
        let findings = default_check("class C {\n    void M() { var v = new List<int>(); }\n}\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, CODE_PERF001);
        assert_eq!(findings[0].severity, Severity::Hint);
        assert_eq!(findings[0].item_kind, "creation");
        assert_eq!(findings[0].item_name.as_deref(), Some("List"));
    }

    /// Empty base and extra lists disable every check.
    #[test]
    fn check_should_return_nothing_for_empty_lists() {
        let source = "class C {\n    void M() { var v = new List<int>(); }\n}\n";
        assert!(check(&parse(source).unwrap(), &[], &[]).is_empty());
    }
}
