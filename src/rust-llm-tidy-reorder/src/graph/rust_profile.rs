//! The Rust [`ReorderProfile`]: the reorder engine's reference profile.
//!
//! [`RustProfile`] emits Rust sources in the ten-phase order documented
//! on it, byte-for-byte.
//!
//! [`ReorderProfile`]: super::profile::ReorderProfile

use super::profile::{
    DeclNamePosition, PhaseContext, PhaseStrategy, ReferenceWalk, ReorderProfile,
};
use super::toposort::TieBreak;
use rust_llm_tidy_model::parse::{ItemKind, SourceItem};

/// The Rust profile's reference walk data (tree-sitter-rust node kinds).
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
};

/// The Rust reorder profile: the engine's reference profile.
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
pub struct RustProfile;

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
    use ahash::AHashSet;
    use rust_llm_tidy_model::parse::parse_source;

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
        let parsed = parse_source(source).unwrap();
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
        let parsed = parse_source(source).unwrap();
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
        let parsed = parse_source(source).unwrap();
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
}
