//! Per-language reorder ordering profiles.
//!
//! A [`ReorderProfile`] is the per-language policy the reorder engine
//! consumes instead of a hard-coded item-kind table.
//!
//! It assigns every parsed item an output phase, chooses the ordering
//! strategy within each phase, ranks in-type members, and provides the
//! grammar node-kind data the reference walk matches against.
//!
//! [`RustProfile`] is the engine's reference profile: it emits Rust
//! sources in the ten-phase order documented on it, byte-for-byte.

use super::toposort::TieBreak;
use ahash::AHashSet;
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
        ("function_item", "name"),
        ("struct_item", "name"),
        ("enum_item", "name"),
        ("union_item", "name"),
        ("trait_item", "name"),
        ("type_item", "name"),
        ("const_item", "name"),
        ("static_item", "name"),
        ("mod_item", "name"),
        ("macro_definition", "name"),
        ("enum_variant", "name"),
        // Binding patterns and aliases.
        ("parameter", "pattern"),
        ("let_declaration", "pattern"),
        ("use_as_clause", "alias"),
        ("extern_crate_declaration", "alias"),
        ("for_expression", "pattern"),
    ],
};

/// Per-file context for phase decisions.
///
/// Carries the engine-computed facts a profile may need beyond the item
/// itself.
pub struct PhaseContext<'a> {
    /// Names of macro definitions in the file. Rust macro scoping is
    /// textual, so a local `macro_rules!` invocation routes to the macro
    /// phase (following its definition); languages without macros ignore
    /// this.
    pub macro_names: &'a AHashSet<&'a str>,
}

/// How the engine orders the items assigned to one phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseStrategy {
    /// Items keep their original file order.
    Stable,
    /// Items sort callers before callees; `TieBreak` orders items the
    /// edges do not constrain.
    Dependency(TieBreak),
    /// Macro definitions dependency-sort alphabetically and each definition
    /// is immediately followed by its local invocations (Rust
    /// `macro_rules!` textual scoping).
    MacroDefinitions,
    /// Items pin after the item naming their target type, inherent
    /// implementations before trait implementations (Rust impl blocks).
    ImplsAfterTargetType,
    /// Items subgroup by visibility tier (`pub`, restricted, private), each
    /// group dependency-sorted with an alphabetical tie-break (Rust fns).
    FnsByVisibility,
}

/// Grammar node-kind data for the reference walk.
///
/// One language's tree-sitter node kinds, provided by its reorder profile
/// so [`ReferenceCollector`] walks any grammar's tree without hard-coded
/// kind knowledge.
///
/// Reference positions beyond the declaration matching (path segments,
/// generic arguments, macro invocations) follow the identifier-family
/// node kinds shared by the supported tree-sitter grammars.
///
/// Grammar nodes a language never produces are inert.
///
/// [`ReferenceCollector`]: super::ReferenceCollector
pub struct ReferenceWalk {
    /// Node kinds that declare one item each: the walk pushes the item
    /// matching the node's `name` field and scans the node's body for
    /// references.
    pub declaration_kinds: &'static [&'static str],
    /// `(parent_kind, field)` pairs marking declaration-name positions
    /// that must not record reference edges (item names, binding patterns,
    /// aliases).
    pub decl_name_positions: &'static [(&'static str, &'static str)],
}

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

/// Per-language ordering policy consumed by the reorder engine.
///
/// Implementations are stateless statics ([`Sync`]) so the engine can share
/// one profile across threads.
pub trait ReorderProfile: Sync {
    /// The output phase for a top-level `item`, given the per-file `ctx`.
    ///
    /// The engine emits phases in ascending order; phase numbers are
    /// opaque to the engine and meaningful only to this profile's
    /// [`strategy`].
    ///
    /// [`strategy`]: ReorderProfile::strategy
    fn phase(&self, item: &SourceItem, ctx: &PhaseContext<'_>) -> u32;

    /// How the engine orders the items assigned to `phase`.
    ///
    /// Consulted only for phases that hold items; [`PhaseStrategy::Stable`]
    /// is the safe default for phases a profile does not specialize.
    fn strategy(&self, phase: u32) -> PhaseStrategy;

    /// The output phase for an in-type member `kind`.
    ///
    /// Member phases share the ascending-order rule of
    /// [`phase`]. The Rust parse emits no members,
    /// so the Rust profile leaves this unused.
    ///
    /// [`phase`]: ReorderProfile::phase
    fn member_phase(&self, kind: ItemKind) -> u32;

    /// How the engine orders the members assigned to member `phase`.
    ///
    /// Members honor [`PhaseStrategy::Stable`] and
    /// [`PhaseStrategy::Dependency`]; any other strategy falls back to
    /// `Stable`.
    fn member_strategy(&self, phase: u32) -> PhaseStrategy;

    /// The grammar node-kind data for this language's reference walk.
    fn reference_walk(&self) -> &'static ReferenceWalk;
}

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
            | ItemKind::Destructor => 1,
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
