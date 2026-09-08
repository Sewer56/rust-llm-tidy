//! `MOD003`: flag paths that include the full namespace.
//!
//! A qualified name spells out where a name lives, such as `System.Console`.
//! This rule reads the file's syntax and emits hints; it does not rewrite code
//! or ask the compiler to resolve names.
//!
//! # Explanation 1: reuse an existing import
//!
//! A plain `using` lets the replacement omit the namespace prefix.
//!
//! The walker tracks imports and names in nested scopes, then looks for the
//! longest import matching the beginning of a name.
//!
//! ```csharp
//! using System.Threading.Tasks;
//!
//! System.Threading.Tasks.Task.Delay(1);
//! Task.Delay(1);
//! ```
//!
//! Here, `using System.Threading.Tasks;` makes `Task` available without its
//! namespace. An alias supplies a replacement name: `using Log = System.Console;`
//! allows `Log.WriteLine("before")`.
//!
//! # Explanation 2: suggest a missing import
//!
//! Without a matching import, the rule suggests a namespace `using` at namespace
//! or file scope, subject to checking that the prefix is a namespace.
//!
//! Type positions suggest importing the parent; expressions suggest importing
//! only the root, preserving possible type and property segments.
//!
//! ```csharp
//! // Before: no import is needed for the long spelling.
//! class C { System.Text.StringBuilder Create() => new(); }
//! ```
//!
//! ```csharp
//! // After: import the namespace, not the type.
//! using System.Text;
//!
//! class C { StringBuilder Create() => new(); }
//! ```
//!
//! Syntax does not distinguish namespaces from containing types. If the proposed
//! prefix is a type, use a type alias or retain qualification instead.
//!
//! `global::` explicitly roots a path. Unshadowed `System` and `Microsoft`
//! are known roots. Other unprefixed roots remain exempt: imports and type
//! positions do not prove that a name includes its full namespace.
//!
//! # Module layout
//!
//! - [`walker`]: traversal, occurrence recording, and suggestions
//! - [`scope`]: the scope-frame data model and its queries
//! - [`usings`]: `using`-directive import collection
//! - [`names`]: dotted-chain segments and bound-name extraction
//! - [`syntax`]: tree-shape predicates and name-leaf readers
//!
//! # Code walkthrough: start at `check`
//!
//! Read these functions in call order, not their order in the file.
//!
//! 1. [`check`] receives an already-parsed file. It creates a
//!    [`walker::Walker`], visits the syntax tree, and returns the diagnostics.
//! 2. [`walker::Walker::collect_relative_roots`] finds namespace components
//!    that could make a known root relative rather than absolute.
//! 3. [`walker::Walker::walk`] visits syntax nodes recursively. Entering
//!    a scope pushes a [`scope::ScopeFrame`]; leaving a nested scope pops it.
//! 4. [`syntax::is_chain_head`] selects the outermost node of a dotted chain. For
//!    `System.Console.WriteLine`, this avoids separate hints for shorter prefixes.
//!    [`walker::Walker::record_occurrence`] then checks the name's eligibility.
//! 5. [`scope::covering_import`] finds the longest visible import prefix.
//!    [`walker::Walker::suggestion_under`] removes a plain import's prefix or replaces
//!    an aliased prefix with its alias.
//! 6. [`walker::Walker::record_occurrence`] adds a hint only when it has advice.
//!    [`walker::Walker::enclosing_item`] supplies the containing item's kind and name;
//!    the name node supplies the line number. [`check`] returns these hints.
//!
//! ## What the stored data means
//!
//! - [`walker::Walker`]: source bytes, relative roots, scopes, and hints
//! - [`scope::ScopeFrame`]: imports and bound names in one active scope
//! - [`scope::Import`]: an imported path, its name, and whether it is an alias
//! - [`scope::Binding`]: a declared name and the position where it starts counting
//! - `walker::Suggestion`: the import's name for the hint and the replacement text
//!
//! The scope stack models nested visibility, not compiler name resolution.
//!
//! [`scope::frame_mentions`] asks whether a scope uses a name;
//! [`scope::frame_shadows`] asks whether it conflicts with the import.
//!
//! Imports and declaration names are collected before visiting a scope's children.
//! A binding's `start` value distinguishes scope-wide declarations from locals
//! that count only from their position.
//!
//! Without a covering import, `record_occurrence` builds the namespace advice
//! shown in Explanation 2. It also rejects roots bound to a local or declaration.
//!
//! ## Trace the first example
//!
//! `usings::collect_usings` records `using System.Threading.Tasks;` as a
//! plain import.
//!
//! `names::name_segments` splits the path into `System`, `Threading`,
//! `Tasks`, `Task`, and `Delay`.
//!
//! The covering import matches `System.Threading.Tasks`. `suggestion_under`
//! removes that prefix, leaving `Task.Delay` if the short name does not conflict.
//!
//! Next: open [`check`], then follow [`walker::Walker::walk`] to
//! [`walker::Walker::record_occurrence`] with this example in mind.
//!
//! # Remarks
//!
//! Hints are withheld for conflicting short names, unknown expression receivers,
//! conditional methods or regions, attributes, import text, and generic chains.

use crate::reporting::Diagnostic;
use crate::source::ParseResult;
use std::collections::HashSet;
use walker::Walker;

mod names;
mod scope;
mod syntax;
mod usings;
mod walker;

/// Known namespace roots, unless syntax shows shadowing or relative use.
const ROOT_SEGMENTS: &[&str] = &["System", "Microsoft"];

/// Hint at each eligible name, respecting aliases and conditional compilation.
///
/// Missing imports use an alias to avoid guessing namespace/type boundaries.
/// Attribute, import, ambiguous, and shadowed names are exempt.
/// A conditional region exempts guarded code and its whole containing method.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut walker = Walker {
        bytes: parsed.source.as_bytes(),
        relative_roots: HashSet::new(),
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    let root = parsed.syntax_tree().root_node();
    walker.collect_relative_roots(root);
    walker.walk(root);
    walker.diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;
    use crate::reporting::Severity;
    use crate::rules::lint::CODE_QUALIFIED_PATH;
    use rstest::rstest;

    /// First occurrences provide complete advice, including aliased static calls.
    #[rstest]
    #[case::missing(
        "class C { void M() { System.Console.WriteLine(1); } }",
        "- If `System` is a namespace and the result is clear, add `using System;` at namespace or file scope and use `Console.WriteLine`."
    )]
    #[case::aliased(
        "using Log = System.Console; class C { void M() { System.Console.WriteLine(1); } }",
        "- If clear at the use site, use `Log.WriteLine`; `Log` is already imported."
    )]
    fn check_should_explain_first_occurrence(#[case] source: &str, #[case] advice: &str) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Hint);
        assert_eq!(
            diagnostics[0].message,
            format!(
                "path `System.Console.WriteLine` includes the full namespace.\n\
                 Why: full namespace prefixes give readers longer lines to scan before reaching the item name, making code harder to understand.\n\
                 Suggestions:\n\
                 {advice}\n\
                 - Retain namespace or type context when needed; use a type alias if the proposed import targets a containing type, not a namespace.\n\
                 - Keep the full path if shortening would reduce clarity or create a name conflict.\n\
                 - Verify the shorter path resolves to the same symbol; this hint uses syntax, not compiler name resolution."
            )
        );
    }

    /// Type advice prefers namespace imports without assuming parents are namespaces.
    #[rstest]
    #[case::string_builder(
        "class C { System.Text.StringBuilder Create() => new(); }",
        "System.Text",
        "StringBuilder"
    )]
    #[case::nested_type(
        "namespace N { class Outer { public class Inner {} } } class C { global::N.Outer.Inner field; }",
        "global::N.Outer",
        "Inner"
    )]
    fn check_should_condition_namespace_advice_on_parent_kind(
        #[case] source: &str,
        #[case] namespace: &str,
        #[case] replacement: &str,
    ) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(&format!(
            "- If `{namespace}` is a namespace and the result is clear, add `using {namespace};` at namespace or file scope and use `{replacement}`."
        )));
        assert!(diagnostics[0].message.contains(
            "use a type alias if the proposed import targets a containing type, not a namespace."
        ));
    }

    /// Static property chains retain their type and property segments.
    #[test]
    fn check_should_keep_static_property_tail_in_replacement() {
        let diagnostics = lint("class C { void M() { System.Console.Out.WriteLine(1); } }");

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("add `using System;`"));
        assert!(
            diagnostics[0]
                .message
                .contains("use `Console.Out.WriteLine`")
        );
    }

    /// Unknown and partial roots stay exempt regardless of depth or type usage.
    #[rstest]
    #[case::type_position("class C { Vendor.Net.Client field; }", 0)]
    #[case::generic_type_position("class C { Vendor.Net.Widget<int> field; }", 0)]
    #[case::namespace(
        "namespace Vendor.Net { class C { void M() { Vendor.Net.Client.Open(); } } }",
        0
    )]
    #[case::file_scoped_namespace(
        "namespace Vendor.Net;\nclass C { void M() { Vendor.Net.Client.Open(); } }",
        0
    )]
    #[case::unknown_receiver("class C { void M() { Vendor.Net.Client.Open(); } }", 0)]
    #[case::ordinary_member("class C { void M() { obj.Member.Open(); } }", 0)]
    #[case::shadowed("class Client {} class C { global::Vendor.Net.Client field; }", 0)]
    #[case::later_shadow("class C { global::Vendor.Net.Client field; } class Client {}", 0)]
    #[case::partial_type("using System; class C { Threading.Tasks.Task field; }", 0)]
    #[case::partial_expression(
        "using System; class C { void M() { Threading.Tasks.Task.Factory.StartNew(); } }",
        0
    )]
    #[case::partial_import(
        "namespace Work { using Threading.Tasks; class C { Threading.Tasks.Task field; } }",
        0
    )]
    #[case::unknown_import("using Vendor.Net; class C { Vendor.Net.Client field; }", 0)]
    #[case::unknown_static_import(
        "using static Vendor.Net.Client; class C { void M() { Vendor.Net.Client.Open(); } }",
        0
    )]
    #[case::nested_namespace(
        "namespace Work { namespace Vendor.Net {} class C { Vendor.Net.Client field; } }",
        0
    )]
    #[case::known_type("class C { System.Threading.Tasks.Task field; }", 1)]
    #[case::microsoft("class C { Microsoft.Win32.RegistryKey field; }", 1)]
    #[case::absolute_custom("class C { global::Vendor.Net.Client field; }", 1)]
    #[case::long_absolute(
        "class C { void M() { global::Vendor.Net.Client.Factory.Instance.Open(); } }",
        1
    )]
    #[case::receiver_shadow(
        "using System.Threading.Tasks; class C { void M(object System) { System.Threading.Tasks.Task.Delay(1); } }",
        0
    )]
    fn check_should_distinguish_custom_roots(#[case] source: &str, #[case] expected: usize) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), expected);
    }

    /// Aliases and relative namespace members are not global roots.
    #[rstest]
    #[case::alias("using System = Vendor.Net; class C { System.Client field; }")]
    #[case::global_using_alias(
        "global using System = Vendor.Net; class C { System.Client field; }"
    )]
    #[case::alias_qualified("using S = System; class C { S::Threading.Tasks.Task field; }")]
    #[case::extern_alias("extern alias System; class C { System::Threading.Tasks.Task field; }")]
    #[case::alias_dotted("using S = System; class C { S.Threading.Tasks.Task field; }")]
    #[case::generic_alias("using System = Vendor.Container<int>; class C { System.Nested field; }")]
    #[case::nested_namespace(
        "namespace Work { namespace System.Threading {} class C { System.Threading.Task field; } }"
    )]
    #[case::separate_namespace(
        "namespace Work.System.Threading {} namespace Work { class C { System.Threading.Task field; } }"
    )]
    #[case::file_scoped_namespace(
        "namespace Work.System; class C { System.Threading.Tasks.Task field; }"
    )]
    #[case::escaped_namespace(
        "namespace Work.@System {} namespace Work { class C { System.Console field; } }"
    )]
    #[case::type_parameter("class C<System> { System.Threading.Tasks.Task field; }")]
    #[case::escaped_binding("class C { void M(object @System) { System.Console.WriteLine(1); } }")]
    #[case::type_binding("class System {} class C { System.Console field; }")]
    #[case::root_import_shadowed(
        "using System; class C { void M(int Console) { System.Console.WriteLine(1); } }"
    )]
    #[case::ambiguous_absolute_alias(
        "using C = global::System.Console; using C = System.Console; class X { global::System.Console field; }"
    )]
    fn check_should_exempt_aliases_and_relative_roots(#[case] source: &str) {
        let diagnostics = lint(source);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    /// Absolute advice keeps its root even when a relative root is shadowed.
    #[rstest]
    #[case::type_path(
        "class C { global::Vendor.Net.Client field; }",
        "global::Vendor.Net.Client",
        "global::Vendor.Net",
        "Client"
    )]
    #[case::expression_path(
        "class C { void M(object System) { global::System.Console.Out.WriteLine(1); } }",
        "global::System.Console.Out.WriteLine",
        "global::System",
        "Console.Out.WriteLine"
    )]
    #[case::relative_import(
        "namespace Work { using Vendor.Net; class C { global::Vendor.Net.Client field; } }",
        "global::Vendor.Net.Client",
        "global::Vendor.Net",
        "Client"
    )]
    fn check_should_preserve_absolute_import_advice(
        #[case] source: &str,
        #[case] path: &str,
        #[case] import: &str,
        #[case] replacement: &str,
    ) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            format!(
                "path `{path}` includes the full namespace.\n\
                 Why: full namespace prefixes give readers longer lines to scan before reaching the item name, making code harder to understand.\n\
                 Suggestions:\n\
                 - If `{import}` is a namespace and the result is clear, add `using {import};` at namespace or file scope and use `{replacement}`.\n\
                 - Retain namespace or type context when needed; use a type alias if the proposed import targets a containing type, not a namespace.\n\
                 - Keep the full path if shortening would reduce clarity or create a name conflict.\n\
                 - Verify the shorter path resolves to the same symbol; this hint uses syntax, not compiler name resolution."
            )
        );
    }

    /// Generic chains are withheld instead of suggesting code without type arguments.
    #[rstest]
    #[case::type_path("class C { System.Collections.Generic.List<int> field; }")]
    #[case::absolute_type("class C { global::System.Collections.Generic.List<int> field; }")]
    #[case::method("class C { void M() { System.Array.Empty<int>(); } }")]
    #[case::nested_type("class C { global::Vendor.Container<int>.Nested field; }")]
    #[case::imported(
        "using System.Collections.Generic; class C { System.Collections.Generic.List<int> field; }"
    )]
    fn check_should_exempt_generic_chains(#[case] source: &str) {
        let diagnostics = lint(source);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    /// Conditional compilation exempts the containing method, not its sibling.
    #[rstest]
    #[case::body("void M() { System.Console.WriteLine(1);\n#if DEBUG\nint x = 1;\n#endif\n}")]
    #[case::nested("void M() { System.Console.WriteLine(1); {\n#if DEBUG\nint x = 1;\n#endif\n} }")]
    #[case::method("\n#if DEBUG\nvoid M() { System.Console.WriteLine(1); }\n#endif\n")]
    #[case::alternative(
        "\n#if DEBUG\nvoid M() { System.Console.WriteLine(1); }\n#else\nvoid M() { System.Console.WriteLine(2); }\n#endif\n"
    )]
    #[case::local_function(
        "void M() { System.Console.WriteLine(1); void Local() {\n#if DEBUG\nint x = 1;\n#endif\n} }"
    )]
    fn check_should_exempt_conditional_method(#[case] guarded: &str) {
        let source =
            format!("class C {{ {guarded}\nvoid Sibling() {{ System.Console.WriteLine(2); }} }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].item_name.as_deref(), Some("Sibling"));
    }

    /// A region guarding a type exempts all its methods, not the next type.
    #[test]
    fn check_should_exempt_conditionally_compiled_type() {
        let source = "#if DEBUG\nclass C { void M() { System.Console.WriteLine(1); } }\n#endif\nclass D { void Sibling() { System.Console.WriteLine(2); } }";

        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].item_name.as_deref(), Some("Sibling"));
    }

    /// Directive-like text is not a preprocessor region.
    #[rstest]
    #[case::comment("// #if DEBUG\n")]
    #[case::string("var text = \"#if DEBUG\";")]
    #[case::verbatim("var text = @\"#if DEBUG\";")]
    fn check_should_ignore_directive_text(#[case] text: &str) {
        let source = format!("class C {{ void M() {{ {text} System.Console.WriteLine(1); }} }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    /// Run MOD003 over a retained parse of `source`.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = parse(source).expect("test source must parse");
        assert!(!parsed.syntax_tree().root_node().has_error());
        check(&parsed)
    }

    // ── Import available ──

    // Plain using + a qualified occurrence -> one Hint naming the
    // short name at the occurrence's line, carrying the enclosing
    // item's kind and name.
    #[test]
    fn fires_when_fully_qualified_path_is_already_imported() {
        let diags = lint(concat!(
            "using System.Threading.Tasks;\n",
            "class C {\n",
            "    void M() { System.Threading.Tasks.Task.Delay(1); }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_QUALIFIED_PATH);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("M"));
        assert!(
            diags[0]
                .message
                .contains("`System.Threading.Tasks.Task.Delay`")
        );
        assert!(diags[0].message.contains("`Tasks` is already imported"));
        assert!(diags[0].message.contains("use `Task.Delay`"));
    }

    // A qualified type position (return type) counts as one occurrence.
    #[test]
    fn fires_for_qualified_type_positions() {
        let diags = lint(concat!(
            "using global::Nq.Text;\n",
            "class C { global::Nq.Text.Widget Make() { return null; } }\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Text` is already imported"));
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("Make"));
    }

    // A field's qualified type reports the field itself as the
    // enclosing item, matching the parser's first-declared-variable
    // rule.
    #[test]
    fn field_qualified_type_reports_the_field_as_enclosing_item() {
        let diags = lint(concat!(
            "using global::Nq.Text;\n",
            "class C { global::Nq.Text.Widget field; }\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].item_kind, "const");
        assert_eq!(diags[0].item_name.as_deref(), Some("field"));
    }

    // Aliased using -> the hint suggests the alias, not the path's
    // last segment.
    #[test]
    fn fires_with_the_alias_when_using_renames() {
        let diags = lint(concat!(
            "using Widget = global::Nq.Text.Widget;\n",
            "class C {\n",
            "    global::Nq.Text.Widget Make() { return null; }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`global::Nq.Text.Widget`"));
        assert!(diags[0].message.contains("`Widget` is already imported"));
        assert!(diags[0].message.contains("use `Widget`"));
    }

    // An aliased using covering a prefix suggests the alias plus the
    // surviving tail.
    #[test]
    fn fires_with_the_alias_plus_tail_when_an_alias_covers_a_prefix() {
        let diags = lint(concat!(
            "using W = global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() { var a = global::Nq.Text.Widget.Empty; }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`global::Nq.Text.Widget.Empty`"));
        assert!(diags[0].message.contains("`W` is already imported"));
        assert!(diags[0].message.contains("use `W.Empty`"));
    }

    // A file-level using stays in scope inside nested namespaces.
    #[test]
    fn fires_in_nested_namespaces_under_a_file_level_using() {
        let diags = lint(concat!(
            "using global::Nq.Text;\n",
            "namespace Work {\n",
            "    class C { void M() { var a = global::Nq.Text.Widget.Empty; } }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Text` is already imported"));
    }

    // An out-of-scope using does not replace missing-import advice.
    #[test]
    fn check_should_suggest_import_when_existing_using_is_out_of_scope() {
        let diags = lint(concat!(
            "namespace Inner { using global::Nq.Text; }\n",
            "namespace Work {\n",
            "    class C { void M() { var a = global::Nq.Text.Widget.Empty; } }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `using global::Nq;`"));
    }

    // A plain using covering the exact path has no advice: it opens
    // the namespace, and the namespace's own name is not a member.
    #[test]
    fn silent_when_a_plain_using_covers_the_path_exactly() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C { void M() { var a = global::Nq.Text.Widget; } }\n",
        ));

        assert!(diags.is_empty());
    }

    // ── Missing imports ──

    // Every occurrence names its path and a valid alias directive.
    //
    // The receiver chain `System.Console` is a link inside each head,
    // so it never counts as a second occurrence (no double counting).
    #[test]
    fn check_should_hint_at_every_occurrence() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        System.Console.WriteLine(1);\n",
            "        System.Console.WriteLine(2);\n",
            "        System.Console.WriteLine(3);\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("M"));
        assert!(diags[0].message.contains("`System.Console.WriteLine`"));
        assert!(diags[0].message.contains("add `using System;`"));
    }

    // Existing imports affect advice, not the number of hints.
    #[test]
    fn check_should_hint_at_every_imported_occurrence() {
        let diags = lint(concat!(
            "using System.Threading.Tasks;\n",
            "class C {\n",
            "    void M() {\n",
            "        System.Threading.Tasks.Task.Delay(1);\n",
            "        System.Threading.Tasks.Task.Delay(2);\n",
            "        System.Threading.Tasks.Task.Delay(3);\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 3);
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("`Tasks` is already imported"))
        );
        assert!(diags.iter().all(|d| !d.message.contains("times")));
    }

    // Hints follow occurrence order across different paths.
    #[test]
    fn check_should_order_hints_by_occurrence() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        System.Console.WriteLine(1);\n",
            "        System.Console.WriteLine(2);\n",
            "        System.Console.WriteLine(3);\n",
            "        System.Math.Max(1, 2);\n",
            "        System.Math.Max(1, 2);\n",
            "        System.Math.Max(1, 2);\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 6);
        assert!(diags[0].message.contains("`System.Console.WriteLine`"));
        assert!(diags[3].message.contains("`System.Math.Max`"));
        assert!(diags[0].line < diags[1].line);
    }

    // Same-line hints follow source order, not alphabetical path order.
    #[test]
    fn check_should_order_same_line_hints_by_occurrence() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        var t = (System.Math.Abs(-1), System.Console.ReadKey());\n",
            "        var u = (System.Math.Abs(-1), System.Console.ReadKey());\n",
            "        var v = (System.Math.Abs(-1), System.Console.ReadKey());\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 6);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[1].line, 3);
        assert!(diags[0].message.contains("`System.Math.Abs`"));
        assert!(diags[1].message.contains("`System.Console.ReadKey`"));
    }

    // Multi-segment chains without a rooted first segment (member
    // receivers) never count.
    #[test]
    fn silent_for_paths_without_a_rooted_first_segment() {
        let diags = lint(concat!(
            "class C {\n",
            "    void M() {\n",
            "        Deep.Member.Op();\n",
            "        Deep.Member.Op();\n",
            "        Deep.Member.Op();\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // ── Exemptions (contract D7) ──

    // Attribute spans never count, including the attribute's qualified name.
    #[test]
    fn silent_inside_attribute_list_spans() {
        let diags = lint(concat!(
            "using global::Nq;\n",
            "[global::Nq.Probe.Mark]\n",
            "class A { }\n",
            "[global::Nq.Probe.Mark]\n",
            "class B { }\n",
            "[global::Nq.Probe.Mark]\n",
            "class C { }\n",
        ));

        assert!(diags.is_empty());
    }

    // `using` directive text itself is never an occurrence, in any of
    // its shapes.
    #[test]
    fn using_directive_text_is_never_flagged() {
        let diags = lint(concat!(
            "using System.Text;\n",
            "using global::Nq.Text.Widget;\n",
            "using W = global::Nq.Text.Widget;\n",
            "using static System.Math;\n",
            "class C { }\n",
        ));

        assert!(diags.is_empty());
    }

    // Shadowing the referenced name suppresses advice.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_parameter_or_local() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void P(int Empty) { var a = global::Nq.Text.Widget.Empty; }\n",
            "    void L() { var Empty = 1; var b = global::Nq.Text.Widget.Empty; }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A local shadows only from its position: the occurrence before it
    // still fires, the one after stays silent.
    #[test]
    fn fires_before_and_suppresses_after_a_shadowing_local() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() {\n",
            "        var a = global::Nq.Text.Widget.Empty;\n",
            "        var Empty = 1;\n",
            "        var b = global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
        assert!(diags[0].message.contains("`Widget` is already imported"));
    }

    // A foreach or catch designation binds its name for the whole
    // statement, so the occurrences after it stay silent.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_foreach_or_catch_designation() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void F() {\n",
            "        foreach (var Empty in items) { var a = global::Nq.Text.Widget.Empty; }\n",
            "    }\n",
            "    void H() {\n",
            "        try { } catch (Exception Empty) { var b = global::Nq.Text.Widget.Empty; }\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A lambda parameter binds inside the lambda body.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_lambda_parameter() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() {\n",
            "        Func<int, int> f = Empty => global::Nq.Text.Widget.Empty.Value;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A same-named declaration in the file scope shadows the name the
    // advice references.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_same_named_declaration() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class Empty { }\n",
            "class C { void M() { var a = global::Nq.Text.Widget.Empty; } }\n",
        ));

        assert!(diags.is_empty());
    }

    /// Root namespace imports supply partially qualified replacements.
    #[rstest]
    #[case::system(
        "using System; class C { void M() { System.Console.WriteLine(1); } }",
        "Console.WriteLine",
        "System"
    )]
    #[case::absolute(
        "using global::Nq; class C { void M() { var value = global::Nq.Text.Widget.Empty; } }",
        "Text.Widget.Empty",
        "Nq"
    )]
    #[case::global_using(
        "global using System; class C { System.Text.StringBuilder Create() => new(); }",
        "Text.StringBuilder",
        "System"
    )]
    fn check_should_reuse_root_namespace_import(
        #[case] source: &str,
        #[case] replacement: &str,
        #[case] imported: &str,
    ) {
        let diags = lint(source);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(&format!(
            "- If clear at the use site, use `{replacement}`; `{imported}` is already imported."
        )));
    }

    // Shadowed imports never fall back to missing-import advice.
    #[test]
    fn check_should_suppress_advice_when_covering_import_is_shadowed() {
        let diags = lint(concat!(
            "using System.Threading.Tasks;\n",
            "class C {\n",
            "    void M(int Task) {\n",
            "        System.Threading.Tasks.Task.Delay(1);\n",
            "        System.Threading.Tasks.Task.Delay(2);\n",
            "        System.Threading.Tasks.Task.Delay(3);\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A destructuring declaration binds every pattern name.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_deconstruction_pattern() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M() {\n",
            "        var (d, Empty) = (1, 2);\n",
            "        var a = global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // An `is` pattern's designation binds its name from the pattern
    // onward.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_pattern_designation() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void M(object o) {\n",
            "        if (o is int Empty) { var a = global::Nq.Text.Widget.Empty; }\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // Out-arguments, positional-pattern sub-designations, and
    // parenthesized case designations bind their names like locals.
    #[test]
    fn silent_when_short_name_is_shadowed_by_out_var_and_positional_designations() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void P() { Try(out var Empty, out var b); var a = global::Nq.Text.Widget.Empty; }\n",
            "    void Q(object o) { if (o is Pair(int a, int Empty)) { var b = global::Nq.Text.Widget.Empty; } }\n",
            "    void R(object o) { switch (o) { case var (a, Empty): break; } var c = global::Nq.Text.Widget.Empty; }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // Query range variables (`from`, `let`, `into`, `join`) bind their
    // names for the rest of the query.
    #[test]
    fn silent_when_short_name_is_shadowed_by_query_variables() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void F() {\n",
            "        var q = from Empty in xs select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void L() {\n",
            "        var q = from x in xs let Empty = x select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void I() {\n",
            "        var q = from x in xs select x into Empty select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void J() {\n",
            "        var q = from x in xs join Empty in ys on x.K equals ys.K select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "    void K() {\n",
            "        var q = from x in xs join y in ys on x.K equals y.K into Empty select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A typed join's range variable - not its type - binds the name.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_typed_join_range_variable() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void T() {\n",
            "        var q = from x in xs join Foo Empty in ys on x.K equals ys.K select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // Renaming only the join binder flips the occurrence to firing.
    #[test]
    fn fires_when_only_the_join_binder_is_renamed() {
        let diags = lint(concat!(
            "using global::Nq.Text.Widget;\n",
            "class C {\n",
            "    void R() {\n",
            "        var q = from x in xs join y in ys on x.K equals y.K select global::Nq.Text.Widget.Empty;\n",
            "    }\n",
            "}\n",
        ));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_QUALIFIED_PATH);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert!(diags[0].message.contains("`Widget` is already imported"));
    }

    // A top-level declaration also binds its name in the file scope,
    // so the advised short name would conflict.
    #[test]
    fn silent_when_a_top_level_declaration_shares_the_short_name() {
        let diags = lint(concat!(
            "using global::Nq;\n",
            "class Util { }\n",
            "class C {\n",
            "    void M() {\n",
            "        global::Nq.Util.Helper.Run();\n",
            "        global::Nq.Util.Helper.Run();\n",
            "        global::Nq.Util.Helper.Run();\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // A top-level import binding the same short name also blocks the
    // advice: the short name already resolves to that import's path.
    #[test]
    fn silent_when_a_top_level_using_shares_the_short_name() {
        let diags = lint(concat!(
            "using Foo = Other.Ns.Foo;\n",
            "class C {\n",
            "    void M() {\n",
            "        var a = Microsoft.Foo.Text.Value;\n",
            "        var b = Microsoft.Foo.Text.Value;\n",
            "        var c = Microsoft.Foo.Text.Value;\n",
            "    }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // An exact namespace cover and a conflicting using never manufacture advice.
    #[test]
    fn silent_when_short_name_is_ambiguous_across_usings() {
        let diags = lint(concat!(
            "using global::Na.X;\n",
            "using global::Nb.X;\n",
            "class C {\n",
            "    void M() { var a = global::Na.X; var b = global::Na.X; var c = global::Na.X; }\n",
            "}\n",
        ));

        assert!(diags.is_empty());
    }

    // ── Static imports ──

    // Static usings do not supply a known imported name.
    #[test]
    fn check_should_suggest_alias_when_only_static_using_exists() {
        let diags = lint(concat!(
            "using static global::Nq.Thing;\n",
            "class C { void M() { global::Nq.Thing.Op(1); } }\n",
        ));

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `using global::Nq;`"));
    }
}
