//! The narrowing pass: rewrite eligible bare `pub` tokens to the
//! most-restrictive enclosing module visibility.

use super::{ReexportSet, is_bare_pub, parse, visibility_node};
use ahash::AHashSet;
use std::borrow::Cow;
use tree_sitter::Node;

/// Narrow bare `pub` items using cross-file module visibility (the file's
/// effective floor from a [`ModuleTree`]) plus a crate-wide re-export guard
/// ([`ReexportSet`]).
///
/// This is the sole entry point; a standalone file (no crate context) is
/// narrowed with `floor = None` and a per-file re-export set built by the
/// caller.
///
/// Top-level eligible items are narrowed to `floor` (if `Some`); inline `mod {}`
/// bodies are then recursed by `walk()`, so a tighter inline floor still applies
/// transitively.
///
/// `crate_reexports` is consulted for every candidate (named match OR glob
/// sentinel).
///
/// # Skips
///
/// - `pub use` (re-exports are never narrowed).
/// - Items inside `impl` bodies (trait-impl method `pub` is irrelevant).
/// - `macro_rules!` definitions.
///
/// # Idempotency
///
/// Already-narrowed children have a restricted visibility and are skipped on a
/// second run.
///
/// # Allocation
///
/// The input is parsed directly with tree-sitter - only the CST is needed,
/// never the gap-anchored spans the reorder/lint model computes.
///
/// Visibility spans come straight from node byte offsets, so no line/column
/// conversion or `proc_macro2` span hack is required.
///
/// When no child needs narrowing (no restricted-visibility inline module with
/// bare-`pub` children, or an idempotent re-run), the input is borrowed back
/// unchanged ([`Cow::Borrowed`]) with zero allocation.
///
/// Otherwise the rewritten buffer is returned as [`Cow::Owned`].
///
/// # Arguments
///
/// - `source` - the Rust source text to narrow in place.
/// - `floor` - the file's effective floor visibility text (e.g. `"pub(crate)"`),
///   or `None` for a standalone file with no crate context (only inline
///   restricted modules narrow).
/// - `crate_reexports` - the crate-wide re-export guard consulted for every
///   candidate (named match OR glob sentinel).
///
/// # Errors
///
/// tree-sitter performs error recovery, so syntactically invalid Rust still
/// yields a tree (possibly with `ERROR` nodes) rather than a parse error.
///
/// The `Result` is only `Err` with [`anyhow::Error`] when the parser cannot
/// be allocated, the language is not set, or tree-sitter returns no tree.
///
/// [`ModuleTree`]: super::ModuleTree
pub fn narrow_vis_in_tree<'a>(
    source: &'a str,
    floor: Option<&'a str>,
    crate_reexports: &ReexportSet,
) -> anyhow::Result<Cow<'a, str>> {
    let tree = parse(source)?;
    let root = tree.root_node();
    let mut edits: Vec<(usize, usize, Cow<'a, str>)> = Vec::new();
    let names = crate_reexports.names();

    // Top-level items: narrow against the file's tree floor. With `floor = None`
    // (standalone, no crate context) this loop is skipped and only inline mods
    // narrow via `walk()`.
    if let Some(f) = floor {
        let count = root.named_child_count() as u32;
        for i in 0..count {
            let item = root.named_child(i).unwrap();
            narrow_if_eligible_owned(item, f, source, Some(names), &mut edits);
        }
    }

    // Inline-mod recursion: tighter inline floors propagate; floor slices here
    // ARE into `source` (zero-alloc).
    walk(root, floor, source, Some(names), &mut edits);

    if edits.is_empty() {
        Ok(Cow::Borrowed(source))
    } else {
        Ok(Cow::Owned(apply_edits(source, edits)))
    }
}

/// Apply byte edits back-to-front (descending start offset) so earlier offsets
/// stay valid as later regions are rewritten.
///
/// The replacement text is the floor visibility, which is always at least as
/// long as the `pub` token it replaces (`pub` -> `pub(crate)` etc.), so the
/// output grows by a few bytes per edit.
///
/// Capacity is preallocated with that slack to keep `replace_range` from
/// reallocating.
fn apply_edits(source: &str, mut edits: Vec<(usize, usize, Cow<'_, str>)>) -> String {
    edits.sort_by_key(|b| std::cmp::Reverse(b.0));
    let mut out = String::with_capacity(source.len() + edits.len() * 8);
    out.push_str(source);
    for (start, end, repl) in edits {
        out.replace_range(start..end, &repl);
    }
    out
}

/// Same as `narrow_if_eligible` but the floor text is owned (the tree path, not
/// a slice of `source`), so the pushed edit wraps `Cow::Owned(floor.to_string())`.
///
/// Kept as a thin twin to preserve `walk()`'s hot-path `&'a str` borrowing (no
/// allocation).
fn narrow_if_eligible_owned<'a, 'n>(
    item: Node<'n>,
    floor: &'a str,
    source: &'a str,
    reexported: Option<&AHashSet<String>>,
    edits: &mut Vec<(usize, usize, Cow<'a, str>)>,
) {
    let Some(name) = eligible_name(item) else {
        return;
    };
    let Some(vis) = visibility_node(item) else {
        return; // private (no visibility) - not bare `pub`
    };
    if !is_bare_pub(vis) {
        return; // already restricted - idempotent skip
    }
    if let Some(set) = reexported
        && let Ok(n) = name.utf8_text(source.as_bytes())
        && (set.contains(n) || set.contains("*"))
    {
        return;
    }
    edits.push((
        vis.start_byte(),
        vis.end_byte(),
        Cow::Owned(floor.to_string()),
    ));
}

/// Recursively walk items, descending only into inline `mod_item` bodies.
///
/// `floor` is the verbatim visibility text of the innermost restricted ancestor
/// module (e.g. `"pub(crate)"`), or `None` at the crate root / inside a
/// non-restricted ancestor.
///
/// Children of a restricted module are narrowed in place; the walk then
/// recurses so the floor propagates transitively.
fn walk<'a, 'n>(
    container: Node<'n>,
    floor: Option<&'a str>,
    source: &'a str,
    reexported: Option<&AHashSet<String>>,
    edits: &mut Vec<(usize, usize, Cow<'a, str>)>,
) {
    let count = container.named_child_count() as u32;
    for i in 0..count {
        let item = container.named_child(i).unwrap();
        if item.kind() != "mod_item" {
            continue;
        }
        let Some(body) = item.child_by_field_name("body") else {
            continue; // `mod foo;` (file form) - no inline body to narrow
        };

        // A restricted-visibility inline module becomes the new floor; a `pub`
        // or private module inherits the enclosing floor (transitive).
        let new_floor = match visibility_node(item) {
            Some(v) if v.named_child_count() >= 1 => Some(&source[v.start_byte()..v.end_byte()]),
            _ => floor,
        };

        let m = body.named_child_count() as u32;
        for j in 0..m {
            let child = body.named_child(j).unwrap();
            narrow_if_eligible(child, new_floor, source, reexported, edits);
        }
        walk(body, new_floor, source, reexported, edits);
    }
}

/// If `item` is a bare-`pub` child under a restricted-visibility floor, push an
/// edit replacing the `pub` token with the floor's visibility text.
///
/// The replacement text borrows `source` (the floor is already a `&str` slice
/// of it), so no per-edit [`String`] is allocated.
///
/// The re-export guard's name is materialized lazily: only when a `pub use`
/// exists AND the child is bare `pub`, so the common path allocates nothing.
fn narrow_if_eligible<'a, 'n>(
    item: Node<'n>,
    floor: Option<&'a str>,
    source: &'a str,
    reexported: Option<&AHashSet<String>>,
    edits: &mut Vec<(usize, usize, Cow<'a, str>)>,
) {
    let Some(floor) = floor else {
        return;
    };
    let Some(name) = eligible_name(item) else {
        return;
    };
    let Some(vis) = visibility_node(item) else {
        return; // private - not bare `pub`
    };
    if !is_bare_pub(vis) {
        return; // already restricted - idempotent skip
    }

    // Re-export guard: skip items whose name is re-exported via `pub use`. A
    // glob sentinel ("*") disables narrowing for every named child. The name
    // is only read when a re-export set exists (rare path).
    if let Some(set) = reexported
        && let Ok(n) = name.utf8_text(source.as_bytes())
        && (set.contains(n) || set.contains("*"))
    {
        return;
    }

    edits.push((vis.start_byte(), vis.end_byte(), Cow::Borrowed(floor)));
}

/// The `name` field node of an item kind eligible for narrowing (fn, struct,
/// enum, union, type, const, static, mod, trait, extern crate).
///
/// Returns `None` for `use`, `impl`, `macro_rules!`, macro invocations, and
/// other kinds (those are never narrowed).
///
/// Returning the borrowed name node (rather than an owned [`String`]) lets the
/// caller defer - and usually skip - the name allocation, since the name is
/// only read by the rare re-export guard.
#[inline]
fn eligible_name<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let name = node.child_by_field_name("name")?;
    match node.kind() {
        "function_item"
        | "struct_item"
        | "enum_item"
        | "union_item"
        | "type_item"
        | "const_item"
        | "static_item"
        | "mod_item"
        | "trait_item"
        | "extern_crate_declaration" => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::narrow_vis_in_tree;
    use crate::backends::rust::visibility::{ParsedFile, ReexportSet, collect_crate_reexports};
    use std::path::PathBuf;

    /// Tree-only and shared-model parsing produce identical narrowed source,
    /// including recovery syntax and re-export guards.
    #[test]
    fn tree_only_parse_should_match_backend_narrowed_output() {
        use crate::backends::{LanguageBackend, rust::RustBackend};

        let cases = [
            ("no edits", "pub fn f() {}"),
            ("narrowing", "pub(crate) mod m { pub fn f() {} }"),
            (
                "reexport",
                "pub use m::f; pub(crate) mod m { pub fn f() {} }",
            ),
            ("recovery", "pub(crate) mod m { pub fn f() {} fn broken( }"),
        ];

        for (name, source) in cases {
            let parsed = RustBackend.parse(source).unwrap();
            let file = ParsedFile {
                path: src(),
                source: source.to_string(),
                tree: parsed.syntax_tree().clone(),
            };
            let reexports = collect_crate_reexports(std::iter::once(&file));
            let mut edits = Vec::new();

            super::walk(
                file.tree.root_node(),
                None,
                source,
                Some(reexports.names()),
                &mut edits,
            );
            let expected = super::apply_edits(source, edits);
            let actual = narrow(source).unwrap();

            assert_eq!(actual, expected, "{name}");
        }
    }

    fn src() -> PathBuf {
        PathBuf::from("test.rs")
    }

    /// Parse `src` into a [`ParsedFile`] (test helper).
    fn parse(text: &str) -> ParsedFile {
        ParsedFile::new(src(), text.to_string()).expect("test source must parse")
    }

    /// Standalone narrowing through the tree entry point: `floor = None` (no
    /// cross-file context) plus a per-file re-export guard built from the file
    /// itself.
    ///
    /// Exercises the same inline-mod path `walk()` takes; the unit tests below
    /// call it to cover inline narrowing, floors, and the re-export guard.
    fn narrow<'a>(src: &'a str) -> anyhow::Result<std::borrow::Cow<'a, str>> {
        let pf = parse(src);
        let reexports = collect_crate_reexports(std::iter::once(&pf));
        narrow_vis_in_tree(src, None, &reexports)
    }

    #[test]
    fn narrows_pub_inside_pub_crate_mod() {
        let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(crate) fn f"), "narrowed: {out}");
        assert!(!out.contains("pub fn f"), "bare pub gone: {out}");
    }

    #[test]
    fn narrows_struct_const_static() {
        let src = "pub(crate) mod m {\n    pub struct S;\n    pub const C: u32 = 0;\n    pub static G: u32 = 1;\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(crate) struct S"), "{out}");
        assert!(out.contains("pub(crate) const C"), "{out}");
        assert!(out.contains("pub(crate) static G"), "{out}");
    }

    #[test]
    fn leaves_pub_use_untouched() {
        let src = "pub(crate) mod m {\n    pub use crate::x;\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub use crate::x"), "pub use untouched: {out}");
    }

    #[test]
    fn narrows_pub_super_floor() {
        let src = "pub(super) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(super) fn f"), "{out}");
    }

    #[test]
    fn narrows_pub_in_path_floor() {
        let src = "pub(in crate::a) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(in crate::a) fn f"), "{out}");
    }

    #[test]
    fn nested_transitive_floor() {
        // outer is pub(crate); inner is bare `pub` (inherits pub(crate) floor).
        let src = "pub(crate) mod outer {\n    pub mod inner {\n        pub fn f() {}\n    }\n}\n";
        let out = narrow(src).unwrap();
        assert!(
            out.contains("pub(crate) mod inner"),
            "inner mod narrowed: {out}"
        );
        assert!(out.contains("pub(crate) fn f"), "transitive floor: {out}");
    }

    #[test]
    fn skips_macro_rules() {
        let src = "pub(crate) mod m {\n    macro_rules! mac {\n        () => {};\n    }\n}\n";
        let out = narrow(src).unwrap();
        assert_eq!(&*out, src, "macro_rules! must not be narrowed");
    }

    #[test]
    fn skips_impl_methods() {
        // struct narrowed; impl method `pub fn f` untouched (no descent into impl).
        let src = "pub(crate) mod m {\n    pub struct S;\n    impl S {\n        pub fn f() {}\n    }\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(crate) struct S"), "{out}");
        assert!(out.contains("pub fn f"), "impl method pub untouched: {out}");
    }

    #[test]
    fn reexport_guard_skips_narrowing() {
        let src = "pub use m::f;\npub(crate) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(
            out.contains("pub fn f"),
            "re-exported item not narrowed: {out}"
        );
    }

    #[test]
    fn reexport_guard_group() {
        let src = "pub use m::{f, g};\npub(crate) mod m {\n    pub fn f() {}\n    pub fn g() {}\n    pub fn h() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(
            out.contains("pub fn f"),
            "f re-exported, not narrowed: {out}"
        );
        assert!(
            out.contains("pub fn g"),
            "g re-exported, not narrowed: {out}"
        );
        assert!(
            out.contains("pub(crate) fn h"),
            "h not re-exported, narrowed: {out}"
        );
    }

    #[test]
    fn idempotent() {
        let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
        let once = narrow(src).unwrap();
        let twice = narrow(&once).unwrap();
        assert_eq!(once, twice, "narrow must be idempotent");
    }

    #[test]
    fn no_top_level_narrowing() {
        // Top-level pub fn is not narrowed (module visibility is out of scope).
        let src = "pub fn f() {}\n";
        let out = narrow(src).unwrap();
        assert_eq!(&*out, src, "top-level pub not narrowed");
    }

    #[test]
    fn clean_input_is_borrowed() {
        // No restricted-visibility inline module: a no-op, returned borrowed.
        let src = "pub fn f() {}\nfn g() {}\n";
        let out = narrow(src).unwrap();
        assert!(
            matches!(out, std::borrow::Cow::Borrowed(_)),
            "no-op input must be borrowed, got owned"
        );
        assert_eq!(&*out, src);
    }

    #[test]
    fn dirty_input_is_owned() {
        let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(
            matches!(out, std::borrow::Cow::Owned(_)),
            "narrowing input must be owned, got borrowed"
        );
        assert!(out.contains("pub(crate) fn f"));
    }

    #[test]
    fn narrow_vis_in_tree_narrows_top_level_with_floor() {
        // foo.rs top-level bare pub, narrowed by the cross-file pub(crate) floor
        // declared in its parent (a standalone file with no floor is left pub).
        let src = "pub fn f() {}\n";
        let reexports = ReexportSet::new();
        let out = narrow_vis_in_tree(src, Some("pub(crate)"), &reexports).unwrap();
        assert!(
            out.contains("pub(crate) fn f"),
            "top-level pub narrowed by tree floor: {out}"
        );
        assert!(!out.contains("pub fn f"), "bare pub gone: {out}");
    }

    #[test]
    fn narrow_vis_in_tree_respects_crate_reexport_guard() {
        // f is re-exported somewhere in the crate -> must stay pub (soundness).
        // Build via the real builder so no test-only ctor is needed on ReexportSet.
        let reexports =
            collect_crate_reexports(std::iter::once(&parse("pub use crate::foo::f;\n")));
        let src = "pub fn f() {}\n";
        let out = narrow_vis_in_tree(src, Some("pub(crate)"), &reexports).unwrap();
        assert!(
            out.contains("pub fn f"),
            "re-exported item NOT narrowed: {out}"
        );
    }

    #[test]
    fn narrow_vis_in_tree_clean_input_is_borrowed() {
        // No bare-pub eligible child under a floor: no edits -> borrowed.
        let src = "struct S;\n";
        let reexports = ReexportSet::new();
        let out = narrow_vis_in_tree(src, Some("pub(crate)"), &reexports).unwrap();
        assert!(
            matches!(out, std::borrow::Cow::Borrowed(_)),
            "clean tree input must be borrowed, got owned"
        );
    }

    #[test]
    fn narrow_vis_in_tree_idempotent() {
        let src = "pub fn f() {}\n";
        let reexports = ReexportSet::new();
        let once = narrow_vis_in_tree(src, Some("pub(crate)"), &reexports).unwrap();
        // Second pass: `pub(crate) fn f` is already restricted -> no edit.
        let twice = narrow_vis_in_tree(&once, Some("pub(crate)"), &reexports).unwrap();
        assert_eq!(&*once, &*twice, "narrow_vis_in_tree must be idempotent");
    }

    #[test]
    fn narrow_vis_in_tree_inline_mod_inherits_file_floor() {
        // The file's tree floor (pub(crate)) must propagate into inline mods
        // via walk(); an inline `pub fn g` must narrow to pub(crate).
        let src = "pub mod inner {\n    pub fn g() {}\n}\n";
        let reexports = ReexportSet::new();
        let out = narrow_vis_in_tree(src, Some("pub(crate)"), &reexports).unwrap();
        assert!(
            out.contains("pub(crate) fn g"),
            "inline child must inherit the file's tree floor: {out}"
        );
        assert!(!out.contains("pub fn g"), "bare inline pub gone: {out}");
    }

    #[test]
    fn crlf_line_endings_preserved_when_narrowing() {
        // CRLF source: bare `pub fn f` inside a `pub(crate)` inline module is
        // narrowed to `pub(crate) fn f` via a byte-range `replace_range` swap
        // that touches only the `pub` token bytes, so every `\r\n` survives.
        let src = "pub(crate) mod m {\r\n    pub fn f() {}\r\n}\r\n";
        let out = narrow(src).unwrap();
        let owned = out.into_owned();
        assert!(owned.contains("pub(crate) fn f"), "narrowed: {owned}");
        assert!(!owned.contains("pub fn f"), "bare pub gone: {owned}");
        assert_eq!(
            owned.matches('\n').count(),
            owned.matches("\r\n").count(),
            "every newline must be CRLF (no LF flip): {owned:?}"
        );
    }
}
