//! The Rust reorder profile: the ordering policy the engine consumes.
//!
//! [`RustProfile`] emits Rust sources in the ten-phase order documented
//! on it, byte-for-byte, and supplies the grammar's reference-walk data
//! (node kinds, name positions, reference shapes).

use rust_llm_tidy_model::parse::{ItemKind, SourceItem};
use rust_llm_tidy_reorder::graph::{
    DeclNamePosition, PhaseContext, PhaseStrategy, ReferencePosition, ReferenceWalk,
    ReorderProfile, TieBreak,
};

/// The Rust grammar's reference-walk data: declaration node kinds,
/// definition-name positions, reference shapes, and the macro-call
/// marker.
static RUST_REFERENCE_WALK: ReferenceWalk = ReferenceWalk {
    declaration_kinds: &[
        "function_item",
        "struct_item",
        "enum_item",
        "union_item",
        "type_item",
        "const_item",
        "static_item",
        "trait_item",
        "macro_definition",
    ],
    decl_name_positions: &[
        // Item declaration names.
        DeclNamePosition::new("function_item", "name"),
        DeclNamePosition::new("struct_item", "name"),
        DeclNamePosition::new("enum_item", "name"),
        DeclNamePosition::new("union_item", "name"),
        DeclNamePosition::new("trait_item", "name"),
        DeclNamePosition::new("type_item", "name"),
        DeclNamePosition::new("const_item", "name"),
        DeclNamePosition::new("static_item", "name"),
        DeclNamePosition::new("mod_item", "name"),
        DeclNamePosition::new("macro_definition", "name"),
        DeclNamePosition::new("enum_variant", "name"),
        // Binding patterns and aliases.
        DeclNamePosition::new("parameter", "pattern"),
        DeclNamePosition::new("let_declaration", "pattern"),
        DeclNamePosition::new("use_as_clause", "alias"),
        DeclNamePosition::new("extern_crate_declaration", "alias"),
        DeclNamePosition::new("for_expression", "pattern"),
    ],
    reference_positions: &[
        // Bare identifiers name a use directly.
        ReferencePosition::bare("identifier"),
        ReferencePosition::bare("type_identifier"),
        // A scoped path references only its first segment.
        ReferencePosition::path("scoped_identifier", "path"),
        ReferencePosition::path("scoped_type_identifier", "path"),
        // A generic type records its base type, then walks its type
        // arguments for further references.
        ReferencePosition::wrapping("generic_type", "type"),
        // A macro call records its called path; the argument token tree
        // is not scanned.
        ReferencePosition::call("macro_invocation", "macro"),
    ],
    macro_marker_kind: "!",
};

/// The Rust reorder profile: the ordering policy the reorder engine
/// consumes.
///
/// Emits Rust items in ten phases:
///
/// 1. `extern crate` + uncategorized + external macro invocations, stable
/// 2. `use`, stable
/// 3. `mod` (file-based, test or not; inline non-test), stable
/// 4. `macro_rules!` definitions, dependency then alphabetical, each
///    followed by its local invocations
/// 5. `const` + `static`, dependency then alphabetical
/// 6. `struct`/`enum`/`union`/`type`, dependency then alphabetical
/// 7. `trait`, dependency then alphabetical
/// 8. `impl`, inherent before trait, after the matching type
/// 9. `fn`, visibility groups, dependency then alphabetical within
/// 10. inline `#[cfg(test)] mod`, stable, last
pub(super) struct RustProfile;

impl ReorderProfile for RustProfile {
    fn phase(&self, item: &SourceItem, ctx: &PhaseContext<'_>) -> u32 {
        match item.kind() {
            ItemKind::Extern | ItemKind::Other => 1,
            ItemKind::Use => 2,
            ItemKind::Mod => {
                // Only an inline `#[cfg(test)] mod x { ... }` lands last;
                // file-based decls and inline non-test mods stay in the
                // mod phase (rustfmt owns alphabetical order for
                // file-based decls).
                if item.is_test_module() && item.is_inline() {
                    10
                } else {
                    3
                }
            }
            ItemKind::Macro => 4,
            ItemKind::MacroInvocation => {
                let name = item.name().unwrap_or("");
                // Only invocations of a locally-defined macro_rules! follow
                // their definition; external macros (println!,
                // tokio::main, ...) stay in the stable uncategorized
                // bucket.
                if !name.is_empty() && ctx.macro_names.contains(name) {
                    4
                } else {
                    1
                }
            }
            ItemKind::Const | ItemKind::Static => 5,
            ItemKind::Struct | ItemKind::Enum | ItemKind::Union | ItemKind::Type => 6,
            ItemKind::Trait => 7,
            ItemKind::Impl => 8,
            ItemKind::Fn => 9,
            // C-family kinds never occur in a Rust parse; the uncategorized
            // stable bucket keeps this table total.
            ItemKind::Namespace
            | ItemKind::Class
            | ItemKind::Interface
            | ItemKind::Using
            | ItemKind::Property
            | ItemKind::Event
            | ItemKind::Constructor
            | ItemKind::Destructor
            | ItemKind::Delegate
            | ItemKind::Operator
            | ItemKind::Record => 1,
        }
    }

    fn strategy(&self, phase: u32) -> PhaseStrategy {
        match phase {
            4 => PhaseStrategy::MacroDefinitions,
            5..=7 => PhaseStrategy::Dependency(TieBreak::Alphabetical),
            8 => PhaseStrategy::ImplsAfterTargetType,
            9 => PhaseStrategy::FnsByVisibility,
            _ => PhaseStrategy::Stable,
        }
    }

    fn member_phase(&self, _kind: ItemKind) -> u32 {
        // The Rust parse emits no members, so member phases never apply.
        0
    }

    fn member_strategy(&self, _phase: u32) -> PhaseStrategy {
        PhaseStrategy::Stable
    }

    fn reference_walk(&self) -> &'static ReferenceWalk {
        &RUST_REFERENCE_WALK
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LanguageBackend, RustBackend};
    use ahash::{AHashMap, AHashSet};
    use rust_llm_tidy_reorder::graph::{ReferenceCollector, compute_order};
    use rust_llm_tidy_reorder::reorder::Permutation;

    // ── Policy: phases and strategies ─────────────────────────────────

    /// The file's macro-definition names, mirroring the engine's set.
    fn macro_names_of(parsed: &rust_llm_tidy_model::parse::ParseResult) -> AHashSet<&str> {
        parsed
            .items
            .iter()
            .filter(|item| item.kind() == &ItemKind::Macro)
            .filter_map(|item| item.name())
            .collect()
    }

    /// Only an inline `#[cfg(test)] mod` lands in the tail phase; a
    /// file-based test mod and an inline non-test mod stay in the mod
    /// phase.
    #[test]
    fn inline_test_mod_lands_in_tail_phase() {
        let source = concat!(
            "#[cfg(test)]\n",
            "mod tests {}\n",
            "#[cfg(test)]\n",
            "mod file_tests;\n",
            "mod helpers {}\n",
        );
        let parsed = RustBackend.parse(source).unwrap();
        let macro_names = macro_names_of(&parsed);
        let ctx = PhaseContext {
            macro_names: &macro_names,
        };

        assert_eq!(RustProfile.phase(&parsed.items[0], &ctx), 10);
        assert_eq!(RustProfile.phase(&parsed.items[1], &ctx), 3);
        assert_eq!(RustProfile.phase(&parsed.items[2], &ctx), 3);
    }

    /// A local `macro_rules!` invocation routes to the macro phase; an
    /// external invocation stays in the stable uncategorized phase.
    #[test]
    fn macro_invocation_routes_by_local_definition() {
        let source = "a!();\nprintln!(\"x\");\nmacro_rules! a { () => {}; }\n";
        let parsed = RustBackend.parse(source).unwrap();
        let macro_names = macro_names_of(&parsed);
        let ctx = PhaseContext {
            macro_names: &macro_names,
        };

        assert_eq!(RustProfile.phase(&parsed.items[0], &ctx), 4, "local a!()");
        assert_eq!(RustProfile.phase(&parsed.items[1], &ctx), 1, "external");
        assert_eq!(RustProfile.phase(&parsed.items[2], &ctx), 4, "macro def");
    }

    /// Every Rust kind maps to its documented phase.
    #[test]
    fn phases_follow_the_documented_order() {
        let source = concat!(
            "extern crate serde;\n",
            "use std::io;\n",
            "mod m;\n",
            "macro_rules! mac { () => {}; }\n",
            "const C: u8 = 0;\n",
            "static S: u8 = 0;\n",
            "struct St;\n",
            "enum En {}\n",
            "union Un { a: u8 }\n",
            "type Ty = u8;\n",
            "trait Tr {}\n",
            "impl St {}\n",
            "fn f() {}\n",
        );
        let parsed = RustBackend.parse(source).unwrap();
        let macro_names = macro_names_of(&parsed);
        let ctx = PhaseContext {
            macro_names: &macro_names,
        };

        let expected = [1u32, 2, 3, 4, 5, 5, 6, 6, 6, 6, 7, 8, 9];
        for (item, &phase) in parsed.items.iter().zip(&expected) {
            assert_eq!(
                RustProfile.phase(item, &ctx),
                phase,
                "kind {} must map to phase {phase}",
                item.kind()
            );
        }
    }

    /// Phase strategies follow the documented Rust order; unlisted phases
    /// fall back to stable.
    #[test]
    fn strategies_follow_the_documented_order() {
        use PhaseStrategy::{
            Dependency, FnsByVisibility, ImplsAfterTargetType, MacroDefinitions, Stable,
        };

        let cases = [
            (1u32, Stable),
            (2, Stable),
            (3, Stable),
            (4, MacroDefinitions),
            (5, Dependency(TieBreak::Alphabetical)),
            (6, Dependency(TieBreak::Alphabetical)),
            (7, Dependency(TieBreak::Alphabetical)),
            (8, ImplsAfterTargetType),
            (9, FnsByVisibility),
            (10, Stable),
            (99, Stable),
        ];
        for (phase, expected) in cases {
            assert_eq!(RustProfile.strategy(phase), expected, "phase {phase}");
        }
    }

    // ── Engine: macro ordering through the profile ────────────────────

    /// Independent `macro_rules!` definitions are sorted alphabetically.
    #[test]
    fn test_macros_sorted_alphabetically() {
        let source = r#"
            macro_rules! b { () => {}; }
            macro_rules! a { () => {}; }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // Source order: 0 = macro b, 1 = macro a. Alphabetical: a, b.
        assert_eq!(order, vec![1, 0]);
    }

    /// `macro_rules!` definitions sort before functions that invoke them.
    #[test]
    fn test_compute_order_macro_before_function() {
        let source = r#"
            fn b() { a!(); }
            macro_rules! a { () => {}; }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // Source order: 0 = fn b, 1 = macro a. Macro should be first.
        assert_eq!(order, vec![1, 0]);
    }

    /// A top-level macro invocation follows its `macro_rules!` definition,
    /// even when `use`/`static` items sit between them in the source.
    #[test]
    fn test_top_level_invocation_after_def() {
        // Source order: 0 = use, 1 = static, 2 = macro_rules! a, 3 = a!().
        let source = r#"
            use std::fs;
            static COUNT: i32 = 0;
            macro_rules! a { () => {}; }
            a!();
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // Expected phases: use(0), macro def(2), invocation(3), static(1).
        assert_eq!(order, vec![0, 2, 3, 1]);
    }

    /// A top-level invocation of an unknown macro (no local `macro_rules!`)
    /// stays stable like other uncategorized items, not in the macro phase.
    #[test]
    fn test_external_invocation_stable() {
        // Source order: 0 = println!(), 1 = macro_rules! a.
        let source = r#"
            println!("x");
            macro_rules! a { () => {}; }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // println! is external (no local def) -> phase 1, stays first.
        // Then macro def a -> phase 4.
        assert_eq!(order, vec![0, 1]);
    }

    /// Multiple invocations of the same macro preserve their source order and
    /// all follow the definition.
    #[test]
    fn test_multiple_invocations_preserve_source_order() {
        // Source order: 0 = a!(), 1 = b!(), 2 = macro_rules! m.
        let source = r#"
            m!(a);
            m!(b);
            macro_rules! m { ($x:ident) => {}; }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // def(2) first, then invocations in source order: a(0), b(1).
        assert_eq!(order, vec![2, 0, 1]);
    }

    /// Duplicate `macro_rules!` names (a later definition shadows the
    /// earlier one) attach the shared invocation to exactly one definition,
    /// so the order never repeats an index and the permutation validates.
    #[test]
    fn duplicate_macro_names_emit_the_invocation_once() {
        // Source order: 0 = macro m, 1 = macro m (shadowing), 2 = m!().
        let source = r#"
            macro_rules! m { () => {}; }
            macro_rules! m { () => {}; }
            m!();
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // The invocation (2) follows the first definition (0) exactly once.
        assert_eq!(order, vec![0, 2, 1]);
        Permutation::new(parsed.items.len(), order)
            .expect("no index may repeat for duplicate definition names");
    }

    /// A `macro_rules!` body invoking another local macro records a reversed
    /// dependency edge, so the referenced macro is defined first.
    #[test]
    fn test_macro_def_calls_another_macro_def() {
        // Source order: 0 = alpha (calls bravo), 1 = bravo, 2 = alpha!().
        let source = r#"
            macro_rules! alpha {
                () => { bravo!(); };
            }
            macro_rules! bravo {
                () => {};
            }
            alpha!();
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // bravo(1) before alpha(0) (alpha depends on bravo), then invocation(2).
        assert_eq!(order, vec![1, 0, 2]);
    }

    /// A chain of three macro defs (a → b → c) sorts by dependency, not
    /// alphabetical order.
    #[test]
    fn test_macro_chain_dependency() {
        // Source order: 0 = macro a (calls b), 1 = macro b (calls c), 2 = macro c.
        let source = r#"
            macro_rules! a {
                () => { b!(); };
            }
            macro_rules! b {
                () => { c!(); };
            }
            macro_rules! c {
                () => {};
            }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // c(2), b(1), a(0): callees before callers.
        assert_eq!(order, vec![2, 1, 0]);
    }

    // ── Engine: mod phases through the profile ────────────────────────

    /// A file-based `#[cfg(test)] mod x;` declaration stays in the mod phase,
    /// keeping its source position among file-based mods instead of moving to
    /// the end (rustfmt owns its alphabetical placement).
    #[test]
    fn file_based_test_mod_stays_in_mod_phase() {
        // Source order: zeta(0), test_helpers(1), alpha(2).
        let source = r#"
            mod zeta;
            #[cfg(test)] mod test_helpers;
            mod alpha;
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // All three are mods -> phase 3, stable source order; the test mod does
        // not jump to the last (phase 10) position.
        assert_eq!(order, vec![0, 1, 2]);
    }

    /// An inline `#[cfg(test)] mod x { ... }` definition lands last, after all
    /// other phases.
    #[test]
    fn inline_test_mod_lands_last() {
        // Source order: inline test mod(0), alpha(1).
        let source = r#"
            #[cfg(test)] mod tests {
                fn helper() {}
            }
            mod alpha;
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // The file-based mod alpha stays in phase 3 (0-index 1) and the inline
        // test mod goes to phase 10, so it is emitted last.
        assert_eq!(order, vec![1, 0]);
    }

    /// Inline non-test and file-based test mods both stay in the mod phase,
    /// preserving source order.
    #[test]
    fn inline_non_test_and_file_based_test_stay_in_mod_phase() {
        // Source order: file-based test mod(0), inline non-test mod(1).
        let source = r#"
            #[cfg(test)] mod file_tests;
            mod helpers_pub {}
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let order = compute_order(&parsed, &RustProfile).unwrap();

        // Neither is an inline test mod, so both stay in phase 3, source order.
        assert_eq!(order, vec![0, 1]);
    }

    // ── Reference collection through the profile's walk data ──────────

    /// Build a name-to-index map assigning each name a position index in the
    /// order given (decoupled from source item order, so unit tests stay stable).
    fn idx_map(names: &[&'static str]) -> AHashMap<&'static str, usize> {
        names.iter().enumerate().map(|(i, &n)| (n, i)).collect()
    }

    /// Macro references are inverted so the macro definition precedes its use.
    #[test]
    fn test_reference_collector_macro_edge_reversed() {
        let source = r#"
            fn b() { a!(); }
            macro_rules! a { () => {}; }
        "#;

        let parsed = RustBackend.parse(source).unwrap();
        let name_to_idx = idx_map(&["b", "a"]);
        let macro_names: AHashSet<&str> = ["a"].into_iter().collect();

        let tree = parsed.syntax_tree();
        let mut collector =
            ReferenceCollector::new(name_to_idx, macro_names, RustProfile.reference_walk());
        collector.collect(tree, source.as_bytes());
        let edges = collector.into_edges();

        // b(0) calls macro a(1); reversed edge (a=1, b=0).
        assert_eq!(edges, vec![(1, 0)]);
    }

    /// `ReferenceCollector` produces edges for fn-to-fn and fn-to-type references.
    #[test]
    fn test_reference_collector_finds_fn_and_type_refs() {
        let source = r#"
            use std::collections::HashMap;

            struct Foo {
                x: i32,
            }

            impl Foo {
                fn new() -> Self {
                    Foo { x: 0 }
                }
            }

            fn a() {
                let f = Foo::new();
            }

            fn b() {
                a();
            }
        "#;

        // Indices: a=0, b=1, Foo=2 (listed order, decoupled from source).
        let name_to_idx = idx_map(&["a", "b", "Foo"]);

        let parsed = RustBackend.parse(source).unwrap();
        let tree = parsed.syntax_tree();
        let mut collector =
            ReferenceCollector::new(name_to_idx, AHashSet::new(), RustProfile.reference_walk());
        collector.collect(tree, source.as_bytes());
        let edges = collector.into_edges();

        // fn a(0) references Foo(2) (type), fn b(1) references a(0) (fn)
        assert_eq!(edges.len(), 2);
        assert!(edges.contains(&(0, 2)));
        assert!(edges.contains(&(1, 0)));
    }

    /// Walk shapes: a generic type records its base type and every type
    /// argument; a scoped path records only its first segment; a macro
    /// call's arguments are never walked.
    #[test]
    fn walk_shapes_record_bases_type_arguments_and_first_segments_only() {
        let cases: &[(&str, &[&str], &[&str], Vec<(usize, usize)>)] = &[
            // Wrapping: the inner Foo surfaces through the type arguments.
            (
                "struct Foo {}\nfn takes(w: Vec<Foo>) {}\n",
                &["Foo", "takes"],
                &[],
                vec![(1, 0)],
            ),
            // Path: only the first segment records, and `self` names no
            // item, so the later `new` is never probed.
            (
                "fn new() {}\nfn a() { self::new(); }\n",
                &["new", "a"],
                &[],
                vec![],
            ),
            // Call: only the called path records (a reversed marker edge);
            // the `target` argument is not walked.
            (
                "macro_rules! m { ($x:expr) => {}; }\nfn target() {}\nfn caller() { m!(target); }\n",
                &["m", "target", "caller"],
                &["m"],
                vec![(0, 2)],
            ),
        ];
        for (source, names, macros, expected) in cases {
            let parsed = RustBackend.parse(source).unwrap();
            let mut collector = ReferenceCollector::new(
                idx_map(names),
                macros.iter().copied().collect(),
                RustProfile.reference_walk(),
            );
            collector.collect(parsed.syntax_tree(), source.as_bytes());
            assert_eq!(
                collector.into_edges(),
                *expected,
                "unexpected edges for source: {source}"
            );
        }
    }

    /// `ReferenceCollector` records edges for struct-to-struct references.
    #[test]
    fn test_cross_type_dependency() {
        let source = r#"
            struct A {}

            struct B {
                a: A,
            }
        "#;

        // Indices: A=0, B=1 (listed order).
        let name_to_idx = idx_map(&["A", "B"]);

        let parsed = RustBackend.parse(source).unwrap();
        let tree = parsed.syntax_tree();
        let mut collector =
            ReferenceCollector::new(name_to_idx, AHashSet::new(), RustProfile.reference_walk());
        collector.collect(tree, source.as_bytes());
        let edges = collector.into_edges();

        // B(1) references A(0).
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0], (1, 0));
    }

    /// Walk data drives declaration matching: a walk declaring `mod_item`
    /// as a declaration kind records references inside a mod body that the
    /// Rust walk (which skips mods) ignores.
    #[test]
    fn walk_data_drives_declaration_matching() {
        let source = "mod m { fn f() { g(); } }\nfn g() {}\n";
        // Indices: m=0, g=1 (listed order).
        let name_to_idx = idx_map(&["m", "g"]);

        static MOD_WALK: ReferenceWalk = ReferenceWalk {
            declaration_kinds: &["mod_item"],
            decl_name_positions: &[DeclNamePosition::new("mod_item", "name")],
            reference_positions: &[ReferencePosition::bare("identifier")],
            macro_marker_kind: "!",
        };

        let parsed = RustBackend.parse(source).unwrap();
        let tree = parsed.syntax_tree();

        let mut rust_walk = ReferenceCollector::new(
            name_to_idx.clone(),
            AHashSet::new(),
            RustProfile.reference_walk(),
        );
        rust_walk.collect(tree, source.as_bytes());
        assert!(
            rust_walk.into_edges().is_empty(),
            "the Rust walk never pushes a mod body"
        );

        let mut mod_walk = ReferenceCollector::new(name_to_idx, AHashSet::new(), &MOD_WALK);
        mod_walk.collect(tree, source.as_bytes());
        let edges = mod_walk.into_edges();

        // Inside mod m(0), fn f calls g(1): edge (0, 1).
        assert_eq!(edges, vec![(0, 1)]);
    }
}
