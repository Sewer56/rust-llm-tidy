//! `MOD003`: suggest imports without losing call-site context.
//!
//! A qualified path spells out where a name lives, such as `std::sync::Arc`.
//! This rule reads the file's syntax and emits hints; it does not rewrite code
//! or ask the compiler to resolve names.
//!
//! Import advice is conditional on readability: retain a parent module or the
//! full path when a bare name loses meaning at the call site.
//!
//! # Explanation 1: reuse an existing import
//!
//! An import gives a long path a short name.
//!
//! The walker tracks imports and names in nested scopes, then looks for the
//! longest import matching the beginning of a path.
//!
//! ```rust
//! use std::sync::Arc;
//!
//! let before = std::sync::Arc::new(42);
//! let after = Arc::new(42);
//! ```
//!
//! Here, `use std::sync::Arc;` already supplies `Arc`, so the hint replaces
//! only `std::sync::Arc`; `::new` stays. An import alias works the same way:
//! `use std::sync::Arc as Shared;` makes the replacement `Shared::new(42)`.
//!
//! # Explanation 2: suggest a missing import
//!
//! Without a matching import, the rule can suggest adding one at module scope.
//!
//! It uses naming conventions: the first uppercase segment is treated as a
//! type or trait, so later segments stay on the replacement.
//!
//! ```rust
//! // Before: no import is needed for the long spelling.
//! let before = std::sync::Arc::new(42);
//! ```
//!
//! ```rust
//! // After: import the type, not its associated function `new`.
//! use std::sync::Arc;
//!
//! let after = Arc::new(42);
//! ```
//!
//! # Module layout
//!
//! - [`walker`]: traversal, occurrence recording, and suggestions
//! - [`scope`]: the scope-frame data model and its queries
//! - [`imports`]: `use`-declaration collection and import advice
//! - [`syntax`]: tree-shape predicates and path-segment readers
//!
//! # Code walkthrough: start at `check`
//!
//! Read these functions in call order, not their order in the file.
//!
//! 1. [`check`] receives an already-parsed file. It creates a
//!    [`walker::Walker`], visits the syntax tree, and returns the diagnostics.
//! 2. [`ROOT_SEGMENTS`] identifies standard crates and `crate`. Explicit
//!    `extern crate` declarations supply other roots within their scopes.
//! 3. [`walker::Walker::walk`] visits syntax nodes recursively. Entering
//!    a scope pushes a [`scope::ScopeFrame`]; leaving a nested scope pops it.
//! 4. [`syntax::is_chain_head`] selects a path's outermost node, so
//!    `std::sync::Arc::new` yields one hint, not one per prefix.
//!    [`walker::Walker::record_occurrence`] splits it into segments
//!    and checks eligibility.
//! 5. [`scope::covering_import`] finds the longest visible import prefix.
//!    [`walker::Walker::suggestion_under`] builds a replacement when that import's
//!    short name is usable.
//! 6. [`walker::Walker::record_occurrence`] adds a hint only when it has advice.
//!    [`walker::Walker::enclosing_item`] supplies the containing item's kind and name;
//!    the path node supplies the line number. [`check`] returns these hints.
//!
//! ## What the stored data means
//!
//! - [`walker::Walker`]: source bytes, active scopes, and accumulated hints
//! - [`scope::ScopeFrame`]: imports and bound names in one active scope
//! - [`scope::Import`]: a full imported path and its short name or alias
//! - [`scope::Binding`]: a declared name and the position where it starts counting
//! - `walker::Suggestion`: the short name used and the replacement text
//!
//! The scope stack models nested visibility, not compiler name resolution.
//!
//! [`scope::frame_mentions`] asks whether a scope uses a name;
//! [`scope::frame_shadows`] asks whether it conflicts with the import.
//!
//! Imports and item names are collected before visiting a scope's children.
//! Their declarations can appear after their uses. A binding's `start` value
//! distinguishes scope-wide items from locals that count only from their position.
//!
//! Without a covering import, [`imports::use_path`] chooses what
//! to import, as shown in Explanation 2.
//!
//! ## Trace the first example
//!
//! `imports::collect_use` turns `use std::sync::Arc;` into an import
//! whose short name is `Arc`.
//!
//! `syntax::scoped_segments` turns the path into `std`, `sync`, `Arc`, `new`.
//!
//! The covering import matches the first three segments. `suggestion_under`
//! joins `Arc` to the remaining `new`, producing `Arc::new`.
//!
//! Next: open [`check`], then follow [`walker::Walker::walk`] to
//! [`walker::Walker::record_occurrence`] with this example in mind.
//!
//! # Remarks
//!
//! Hints are withheld for conflicting short names and exempt syntax, including
//! imports, macros, attributes, and conditionally compiled regions or functions.
//!
//! Partial paths, `self`/`super`, and uncertain crate roots remain exempt.
//! Absolute `::` paths retain their root in import advice.

use crate::reporting::Diagnostic;
use crate::source::ParseResult;
use walker::Walker;

mod imports;
mod scope;
mod syntax;
mod walker;

/// Known crate roots, unless a visible declaration or import shadows them.
const ROOT_SEGMENTS: &[&str] = &["crate", "std", "core", "alloc"];

/// Hint at each eligible path, respecting aliases and conditional compilation.
///
/// Macro, attribute, import, ambiguous, and shadowed paths are exempt.
/// Conditional attributes exempt guarded items and the whole containing function.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut walker = Walker {
        bytes: parsed.source.as_bytes(),
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    let root = parsed.syntax_tree().root_node();
    walker.walk(root);
    walker.diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;
    use crate::reporting::Severity;
    use crate::rules::lint::CODE_QUALIFIED_PATH;
    use rstest::rstest;

    /// A first occurrence supplies complete import and replacement advice.
    #[rstest]
    #[case::missing(
        "fn f() { std::sync::Arc::new(1); }",
        "std::sync::Arc::new",
        "- If clear at the call site, add `use std::sync::Arc;` at module scope and use `Arc::new`."
    )]
    #[case::imported(
        "use std::sync::Arc; fn f() { std::sync::Arc::new(1); }",
        "std::sync::Arc::new",
        "- If clear at the call site, use `Arc::new`; `Arc` is already imported."
    )]
    #[case::aliased(
        "use std::sync::Arc as Shared; fn f() { std::sync::Arc::new(1); }",
        "std::sync::Arc::new",
        "- If clear at the call site, use `Shared::new`; `Shared` is already imported."
    )]
    #[case::absolute(
        "fn f() { ::vendor::net::Client::new(); }",
        "::vendor::net::Client::new",
        "- If clear at the call site, add `use ::vendor::net::Client;` at module scope and use `Client::new`."
    )]
    #[case::absolute_imported(
        "use ::std::sync::Arc; fn f() { ::std::sync::Arc::new(1); }",
        "::std::sync::Arc::new",
        "- If clear at the call site, use `Arc::new`; `Arc` is already imported."
    )]
    #[case::absolute_crate_import(
        "use ::std; fn f() { ::std::sync::Arc::new(1); }",
        "::std::sync::Arc::new",
        "- If clear at the call site, add `use ::std::sync::Arc;` at module scope and use `Arc::new`."
    )]
    #[case::mixed_full_and_partial(
        "use std::fs; fn f() { std::fs::create_dir_all(\"a\"); fs::create_dir_all(\"b\"); }",
        "std::fs::create_dir_all",
        "- If clear at the call site, use `fs::create_dir_all`; `fs` is already imported."
    )]
    #[case::process_id_missing(
        "fn f() { std::process::id(); }",
        "std::process::id",
        "- If clear at the call site, add `use std::process::id;` at module scope and use `id`."
    )]
    #[case::process_id_imported(
        "use std::process::id; fn f() { std::process::id(); }",
        "std::process::id",
        "- If clear at the call site, use `id`; `id` is already imported."
    )]
    fn check_should_explain_first_occurrence(
        #[case] source: &str,
        #[case] path: &str,
        #[case] advice: &str,
    ) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Hint);
        assert_eq!(
            diagnostics[0].message,
            format!(
                "path `{path}` includes the full namespace.\n\
                 - Shorten with imports only if the meaning remains clear at the call site.\n\
                 {advice}\n\
                 - Import a parent module if the bare name loses context: for example, import `std::process` and use `process::id()`, not `id()`.\n\
                 - Keep the full path if shortening would reduce clarity or create a name conflict."
            )
        );
    }

    /// Only explicit roots receive advice; partial and uncertain paths stay unchanged.
    #[rstest]
    #[case::imported_module("use std::fs; fn f() { fs::create_dir_all(\"a\"); }", 0)]
    #[case::imported_alias("use std::fs as io; fn f() { io::create_dir_all(\"a\"); }", 0)]
    #[case::deep_partial("use std::os; fn f() { os::unix::fs::symlink(\"a\", \"b\"); }", 0)]
    #[case::relative_self("fn f() { self::net::connect(); }", 0)]
    #[case::relative_super("mod m { fn f() { super::net::connect(); } }", 0)]
    #[case::uncertain_type("fn f() { vendor::net::Client::new(); }", 0)]
    #[case::uncertain_function("fn f() { vendor::connect(); }", 0)]
    #[case::uncertain_import("use vendor::net::Client; fn f() { vendor::net::Client::new(); }", 0)]
    #[case::relative_import("use self::vendor; fn f() { vendor::net::Client::new(); }", 0)]
    #[case::associated("fn f() { Client::new(); }", 0)]
    #[case::shadowed("struct Client; fn f() { ::vendor::net::Client::new(); }", 0)]
    #[case::later_shadow("fn f() { ::vendor::net::Client::new(); } struct Client;", 0)]
    #[case::primitive("fn f() { str::to_string(\"a\"); }", 0)]
    #[case::standard_std("fn f() { std::fs::create_dir_all(\"a\"); }", 1)]
    #[case::standard_core("fn f() { core::mem::drop(1); }", 1)]
    #[case::standard_alloc("fn f() { alloc::vec::Vec::new(); }", 1)]
    #[case::crate_root("fn f() { crate::net::connect(); }", 1)]
    #[case::absolute_std("fn f() { ::std::fs::create_dir_all(\"a\"); }", 1)]
    #[case::absolute_vendor("fn f() { ::vendor::net::Client::new(); }", 1)]
    #[case::declared_extern("extern crate vendor; fn f() { vendor::net::Client::new(); }", 1)]
    #[case::later_extern("fn f() { vendor::connect(); } extern crate vendor;", 1)]
    #[case::extern_alias("extern crate vendor as net; fn f() { net::Client::new(); }", 1)]
    #[case::extern_original_after_alias(
        "extern crate vendor as net; fn f() { vendor::Client::new(); }",
        0
    )]
    #[case::local_std("mod std {} fn f() { std::fs::create_dir_all(\"a\"); }", 0)]
    #[case::aliased_std(
        "use crate::local as std; fn f() { std::fs::create_dir_all(\"a\"); }",
        0
    )]
    #[case::raw_local_std("mod r#std {} fn f() { std::fs::create_dir_all(\"a\"); }", 0)]
    #[case::raw_aliased_std(
        "use crate::local as r#std; fn f() { std::fs::create_dir_all(\"a\"); }",
        0
    )]
    #[case::absolute_shadowed_std("mod std {} fn f() { ::std::mem::drop(1); }", 1)]
    #[case::nested_module("mod m { fn f() { std::mem::drop(1); } }", 1)]
    #[case::nested_block("fn f() { { std::mem::drop(1); } }", 1)]
    #[case::nested_module_extern("mod m { extern crate vendor; fn f() { vendor::connect(); } }", 1)]
    #[case::nested_block_extern("fn f() { { extern crate vendor; vendor::connect(); } }", 1)]
    #[case::nested_module_shadow("mod m { mod std {} fn f() { std::mem::drop(1); } }", 0)]
    #[case::nested_block_shadow("fn f() { { use crate::local as std; std::mem::drop(1); } }", 0)]
    #[case::sibling_module_extern(
        "mod m { extern crate vendor; } fn f() { vendor::connect(); }",
        0
    )]
    #[case::sibling_block_extern("fn f() { { extern crate vendor; } vendor::connect(); }", 0)]
    #[case::sibling_function_extern(
        "fn f() { extern crate vendor; } fn g() { vendor::connect(); }",
        0
    )]
    #[case::glob_uncertain_std("use crate::local::*; fn f() { std::mem::drop(1); }", 0)]
    #[case::glob_absolute_std("use crate::local::*; fn f() { ::std::mem::drop(1); }", 1)]
    #[case::conditional_glob("#[cfg(unix)] use crate::local::*; fn f() { std::mem::drop(1); }", 0)]
    #[case::conditional_extern(
        "#[cfg(unix)] extern crate vendor; fn f() { vendor::connect(); }",
        0
    )]
    #[case::conditional_extern_alias(
        "#[cfg(unix)] extern crate vendor as std; fn f() { std::mem::drop(1); }",
        0
    )]
    #[case::type_parameter("fn f<std>() { std::mem::drop(1); }", 0)]
    #[case::raw_type_parameter("fn f<r#std>() { std::mem::drop(1); }", 0)]
    #[case::nested_module_does_not_inherit_extern(
        "extern crate vendor; mod inner { fn f() { vendor::connect(); } }",
        0
    )]
    fn check_should_distinguish_full_and_partial_paths(
        #[case] source: &str,
        #[case] expected: usize,
    ) {
        let diagnostics = lint(source);

        assert_eq!(diagnostics.len(), expected);
    }

    /// Conditional compilation exempts the containing function, not its sibling.
    #[rstest]
    #[case::body("fn f() { std::mem::drop(1); #[cfg(unix)] let x = 1; }")]
    #[case::nested("fn f() { std::mem::drop(1); { #[cfg(unix)] let x = 1; } }")]
    #[case::function("#[cfg(unix)] fn f() { std::mem::drop(1); }")]
    #[case::test_configuration("#[cfg(test)] fn f() { std::mem::drop(1); }")]
    #[case::conditional_attribute("#[cfg_attr(unix, allow(unused))] fn f() { std::mem::drop(1); }")]
    #[case::body_cfg_attr(
        "fn f() { std::mem::drop(1); #[cfg_attr(unix, allow(unused))] let x = 1; }"
    )]
    #[case::module("#[cfg(test)] mod tests { fn f() { std::mem::drop(1); } }")]
    #[case::inner_module("mod tests { #![cfg(test)] fn f() { std::mem::drop(1); } }")]
    #[case::implementation("#[cfg(unix)] impl X { fn f() { std::mem::drop(1); } }")]
    #[case::comment_between("#[cfg(unix)] // guard\nfn f() { std::mem::drop(1); }")]
    fn check_should_exempt_conditional_function(#[case] guarded: &str) {
        let source = format!("{guarded}\nfn sibling() {{ std::mem::drop(2); }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].item_name.as_deref(), Some("sibling"));
    }

    /// Directive-like text is not a conditional attribute.
    #[rstest]
    #[case::comment("// #[cfg(unix)]\n")]
    #[case::string("let text = \"#[cfg(test)]\";")]
    #[case::raw_string("let text = r#\"#[cfg_attr(test, ignore)]\"#;")]
    fn check_should_ignore_directive_text(#[case] text: &str) {
        let source = format!("fn f() {{ {text} std::mem::drop(1); }}");

        let diagnostics = lint(&source);

        assert_eq!(diagnostics.len(), 1);
    }

    /// A cfg-guarded import reserves its name: advising another import
    /// would duplicate it under the active cfg.
    #[test]
    fn check_should_not_advise_import_when_guarded_import_exists() {
        let diagnostics =
            lint("#[cfg(unix)] use std::sync::Arc;\nfn f() { std::sync::Arc::new(1); }");

        assert!(diagnostics.is_empty());
    }

    /// Run MOD003 over a retained parse of `source`.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = parse_source(source).unwrap();
        check(&parsed)
    }

    // ── Import available ──

    // Plain import + qualified occurrence in type position -> one Hint
    // naming the short name at the occurrence's line. It carries the
    // enclosing item's kind and name.
    #[test]
    fn fires_when_fully_qualified_path_is_already_imported() {
        let diags = lint(
            "use std::collections::HashMap;\n\
             fn f() -> std::collections::HashMap {\n\
             Default::default()\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_QUALIFIED_PATH);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("f"));
        assert!(diags[0].message.contains("`std::collections::HashMap`"));
        assert!(diags[0].message.contains("`HashMap` is already imported"));
    }

    // Associated members use the imported type, never an associated-item import.
    #[test]
    fn fires_for_chains_extending_an_imported_prefix() {
        let diags = lint(
            "use std::sync::Arc;\n\
             fn f() {\n\
             let _ = std::sync::Arc::new(1);\n\
             let _ = std::sync::Arc::new(2);\n\
             let _ = std::sync::Arc::new(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("`Arc` is already imported"))
        );
        assert!(diags.iter().all(|d| d.message.contains("use `Arc::new`")));
        assert!(
            diags
                .iter()
                .all(|d| !d.message.contains("use std::sync::Arc::new;"))
        );
    }

    // `self` inside a group binds the group's prefix path: `use
    // crate::a::b::{self}` imports `crate::a::b` as `b`.
    #[test]
    fn self_in_a_group_binds_the_group_prefix() {
        let diags = lint(
            "use crate::a::b::{self};\n\
             fn f() {\n\
             let _ = crate::a::b;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`b` is already imported"));
    }

    // Every group member is its own import -> both occurrences fire.
    #[test]
    fn fires_for_each_member_of_a_grouped_import() {
        let diags = lint(
            "use crate::a::b::{C, D};\n\
             fn f() {\n\
             let _ = (crate::a::b::C, crate::a::b::D);\n\
             }\n",
        );

        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("`C`"));
        assert!(diags[1].message.contains("`D`"));
    }

    // Aliased import -> the hint suggests the alias, not the path's
    // last segment.
    #[test]
    fn fires_with_the_alias_when_import_renames() {
        let diags = lint(
            "use crate::a::b::E as F;\n\
             fn f() {\n\
             let _ = crate::a::b::E;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`crate::a::b::E`"));
        assert!(diags[0].message.contains("`F` is already imported"));
    }

    // A top-level import stays in scope inside nested modules.
    #[test]
    fn fires_in_nested_modules_under_a_top_level_import() {
        let diags = lint(
            "use crate::a::b::C;\n\
             mod tests {\n\
             fn t() {\n\
             let _ = crate::a::b::C;\n\
             }\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`C` is already imported"));
    }

    // An out-of-scope import does not replace missing-import advice.
    #[test]
    fn check_should_suggest_import_when_existing_import_is_out_of_scope() {
        let diags = lint(
            "mod inner {\n\
             pub use crate::a::b::C;\n\
             }\n\
             fn f() {\n\
             crate::a::b::C;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `use crate::a::b::C;`"));
    }

    // A `use` after the occurrence still selects imported-name advice:
    // items are visible throughout their scope, and in-order advice
    // would duplicate that import.
    #[test]
    fn check_should_apply_import_advice_when_use_follows_the_occurrence() {
        let diags = lint("fn f() { std::sync::Arc::new(1); }\nuse std::sync::Arc;");

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Arc` is already imported"));
        assert!(!diags[0].message.contains("Add `use"));
    }

    // ── Missing imports ──

    // Every occurrence names its path and the suggested import.
    #[test]
    fn check_should_hint_at_every_occurrence() {
        let diags = lint(
            "fn f() {\n\
             std::mem::drop(1);\n\
             std::mem::drop(2);\n\
             std::mem::drop(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("f"));
        assert!(diags[0].message.contains("`std::mem::drop`"));
        assert!(diags[0].message.contains("use `drop`"));
        assert!(diags[0].message.contains("use std::mem::drop;"));
    }

    // A path ending in an associated item hoists through its type:
    // rustc rejects `use std::sync::Arc::new;` (E0432), and
    // `use std::sync::Arc;` imports the type.
    #[test]
    fn repeated_associated_item_chain_advises_the_importable_prefix() {
        let diags = lint(
            "fn f() {\n\
             std::sync::Arc::new(1);\n\
             std::sync::Arc::new(2);\n\
             std::sync::Arc::new(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert!(diags[0].message.contains("`std::sync::Arc::new`"));
        assert!(diags[0].message.contains("use std::sync::Arc;"));
        assert!(!diags[0].message.contains("use std::sync::Arc::new;"));
    }

    // Existing imports affect advice, not the number of hints.
    #[test]
    fn check_should_hint_at_every_imported_occurrence() {
        let diags = lint(
            "use std::mem::drop;\n\
             fn f() {\n\
             std::mem::drop(1);\n\
             std::mem::drop(2);\n\
             std::mem::drop(3);\n\
             }\n",
        );

        assert_eq!(diags.len(), 3);
        assert!(
            diags
                .iter()
                .all(|d| d.message.contains("`drop` is already imported"))
        );
        assert!(diags.iter().all(|d| !d.message.contains("times")));
    }

    // ── Exemptions (contract D7) ──

    // Macro spans include the macro's own path, even when imported.
    #[test]
    fn silent_inside_macro_invocation_and_definition_spans() {
        let diags = lint(
            "use log::debug;\n\
             macro_rules! probe {\n\
             () => {\n\
             std::old::Thing::x();\n\
             };\n\
             }\n\
             fn f() {\n\
             log::debug!(\"a\");\n\
             log::debug!(\"b\");\n\
             log::debug!(\"c\");\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // Attribute spans never count, including the attribute's own scoped path.
    #[test]
    fn silent_inside_attribute_spans() {
        let diags = lint(
            "#[std::old::marker::Probe]\n\
             fn a() {}\n\
             #[std::old::marker::Probe]\n\
             fn b() {}\n\
             #[std::old::marker::Probe]\n\
             fn c() {}\n",
        );

        assert!(diags.is_empty());
    }

    // `use` declaration text itself is never an occurrence.
    #[test]
    fn use_declaration_text_is_never_flagged() {
        let diags = lint(
            "use std::collections::HashMap;\n\
             use crate::a::b::{C, D};\n\
             use crate::a::b::E as F;\n",
        );

        assert!(diags.is_empty());
    }

    // A local binding or parameter shadowing the short name suppresses advice.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_local_binding() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn f(C: u8) {\n\
             let _ = crate::a::b::C;\n\
             }\n\
             fn g() {\n\
             let C = 1;\n\
             let _ = crate::a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A same-named item in the file scope shadows the import.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_same_named_item() {
        let diags = lint(
            "use crate::a::b::C;\n\
             struct C;\n\
             fn f() {\n\
             let _ = crate::a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A top-level function also binds its name in the file scope, so a
    // suggested `use` of the same short name would conflict.
    #[test]
    fn silent_when_a_top_level_function_shares_the_short_name() {
        let diags = lint(
            "fn probe() {}\n\
             fn f() {\n\
             let _ = crate::x::probe;\n\
             let _ = crate::x::probe;\n\
             let _ = crate::x::probe;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // The shadowing gate checks the advised import's short name
    // (`Arc`), not the repeated path's last segment (`new`): the
    // import is what would conflict.
    #[test]
    fn silent_when_a_top_level_item_shares_the_advised_import_name() {
        let diags = lint(
            "struct Arc;\n\
             fn f() {\n\
             std::sync::Arc::new(1);\n\
             std::sync::Arc::new(2);\n\
             std::sync::Arc::new(3);\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A `let` shadows only from its position: the occurrence before it
    // still fires, the one after stays silent.
    #[test]
    fn fires_before_and_suppresses_after_a_shadowing_let() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn g() {\n\
             let _ = crate::a::b::C;\n\
             let C = 1;\n\
             let _ = crate::a::b::C;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        assert!(diags[0].message.contains("`C` is already imported"));
    }

    // An if-let pattern binds its name for the rest of the block, so
    // the occurrence after it stays silent.
    #[test]
    fn silent_when_short_name_is_shadowed_by_an_if_let_pattern() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn g() {\n\
             if let C = 1 {}\n\
             let _ = crate::a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A destructuring `let` binds every pattern identifier.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_destructuring_let() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn g() {\n\
             let (d, C) = (1, 2);\n\
             let _ = crate::a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A loop pattern binds inside the loop body.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_loop_pattern() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn g() {\n\
             for C in 0..1 {\n\
             let _ = crate::a::b::C;\n\
             }\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A closure parameter binds inside the closure body.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_closure_parameter() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn g() {\n\
             let h = |C| crate::a::b::C;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A match arm pattern binds inside the arm's value.
    #[test]
    fn silent_when_short_name_is_shadowed_by_a_match_arm_pattern() {
        let diags = lint(
            "use crate::a::b::C;\n\
             fn g() {\n\
             match 1 {\n\
             C => crate::a::b::C,\n\
             _ => 2,\n\
             };\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // Shadowed imports never fall back to missing-import advice.
    #[test]
    fn check_should_suppress_advice_when_covering_import_is_shadowed() {
        let diags = lint(
            "use std::sync::Arc;\n\
             fn f(Arc: u8) {\n\
             let _ = std::sync::Arc::new(1);\n\
             let _ = std::sync::Arc::new(2);\n\
             let _ = std::sync::Arc::new(3);\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // A one-segment import never covers: its suggestion would repeat
    // the path verbatim (`use std;` over `std::mem`).
    #[test]
    fn one_segment_imports_never_cover_a_path() {
        let diags = lint(
            "use std;\n\
             fn f() {\n\
             std::mem::drop(1);\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `use std::mem::drop;`"));
    }

    // Two imports binding one short name make the advice ambiguous:
    // no replacement advice is safe.
    #[test]
    fn silent_when_short_name_is_ambiguous_across_imports() {
        let diags = lint(
            "use crate::a::X;\n\
             use crate::b::X;\n\
             fn f() {\n\
             let _ = crate::a::X;\n\
             let _ = crate::a::X;\n\
             let _ = crate::a::X;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }

    // ── Root resolution and globs ──

    // An absolute root remains certain, but a glob supplies no known short name.
    #[test]
    fn check_should_suggest_explicit_import_when_only_glob_exists() {
        let diags = lint(
            "use ::a::b::*;\n\
             fn f() {\n\
             let _ = ::a::b::C;\n\
             }\n",
        );

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add `use ::a::b::C;`"));
    }

    // Hints follow occurrence order across different paths.
    #[test]
    fn check_should_order_hints_by_occurrence() {
        let diags = lint(
            "fn f() {\n\
             std::mem::drop(1);\n\
             std::mem::drop(2);\n\
             std::mem::drop(3);\n\
             std::io::copy(&mut a, &mut b);\n\
             std::io::copy(&mut a, &mut b);\n\
             std::io::copy(&mut a, &mut b);\n\
             }\n",
        );

        assert_eq!(diags.len(), 6);
        assert!(diags[0].message.contains("`std::mem::drop`"));
        assert!(diags[3].message.contains("`std::io::copy`"));
        assert!(diags[0].line < diags[1].line);
    }

    // Same-line hints follow source order, not alphabetical path order.
    #[test]
    fn check_should_order_same_line_hints_by_occurrence() {
        let diags = lint(
            "fn f(mut a: u8, mut b: u8) {\n\
             let _ = (std::mem::drop(&a), std::io::copy(&mut a, &mut b));\n\
             let _ = (std::mem::drop(&a), std::io::copy(&mut a, &mut b));\n\
             let _ = (std::mem::drop(&a), std::io::copy(&mut a, &mut b));\n\
             }\n",
        );

        assert_eq!(diags.len(), 6);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[1].line, 2);
        assert!(diags[0].message.contains("`std::mem::drop`"));
        assert!(diags[1].message.contains("`std::io::copy`"));
    }

    // Multi-segment paths without a rooted first segment (enum variant
    // paths) never count.
    #[test]
    fn silent_for_paths_without_a_rooted_first_segment() {
        let diags = lint(
            "enum Error { X }\n\
             fn f() {\n\
             let _ = Error::X;\n\
             let _ = Error::X;\n\
             let _ = Error::X;\n\
             }\n",
        );

        assert!(diags.is_empty());
    }
}
