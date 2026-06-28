//! Visibility narrowing for children of restricted-visibility inline modules,
//! via cross-file module-tree resolution.
//!
//! See the crate README (`README.MD`) for the rule, the re-export guard,
//! idempotency, and the crate-aware scope.

#![doc = include_str!(concat!("../", env!("CARGO_PKG_README")))]

use ahash::AHashSet;
pub use modules::{ModuleTree, build_module_tree, discover_crate_root};
use std::borrow::Cow;
use syn::Visibility;
use syn::spanned::Spanned;

mod modules;

/// Crate-wide set of simple names re-exported via `pub use` across every file
/// in a crate, plus the glob sentinel `"*"`.
///
/// Built by [`collect_crate_reexports`]. A glob (`pub use p::*`) in ANY file
/// records `"*"`, which (soundness) disables narrowing for every named child
/// across the crate - matching the per-file glob behavior at a crate scope.
/// This is the conservative default for Open Q6 (cross-file glob scope); a
/// finer-grained per-module-path glob is left as future work.
#[derive(Default)]
pub struct ReexportSet(AHashSet<String>);

impl ReexportSet {
    /// Empty set (no re-exports, no glob).
    pub fn new() -> Self {
        Self::default()
    }

    /// True if `name` is re-exported anywhere in the crate, or if a glob was
    /// seen (the `"*"` sentinel disables narrowing for every named child).
    pub fn blocks(&self, name: &str) -> bool {
        self.0.contains(name) || self.0.contains("*")
    }

    /// True if a `pub use ... ::*` glob was seen in any file.
    pub fn has_glob(&self) -> bool {
        self.0.contains("*")
    }

    /// Read-only access for callers that walk the set directly.
    pub fn names(&self) -> &AHashSet<String> {
        &self.0
    }
}

/// Build a crate-wide [`ReexportSet`] by scanning every parsed file's `pub use`
/// items (top-level and nested in inline modules). Mirrors the shared
/// `collect_reexports` (private, same logic) but unions across all files.
///
/// This is the P3 hard-correctness gate's data source: a missed cross-file
/// re-export turns a safe narrowing into a soundness bug, so the caller MUST
/// pass every `.rs` file in the crate.
pub fn collect_crate_reexports<'a>(files: impl IntoIterator<Item = &'a syn::File>) -> ReexportSet {
    let mut out = ReexportSet::new();
    for file in files {
        let mut per_file: Option<AHashSet<String>> = None;
        collect_reexports(&file.items, &mut per_file);
        if let Some(names) = per_file {
            out.0.extend(names);
        }
    }
    out
}

// `ReexportSet` + `collect_crate_reexports` are defined in this file.

/// Narrow bare `pub` items using cross-file module visibility (the file's
/// effective floor from a [`ModuleTree`]) plus a crate-wide re-export guard
/// ([`ReexportSet`]). This is the sole entry point; a standalone file (no crate
/// context) is narrowed with `floor = None` and a per-file re-export set built
/// by the caller.
///
/// Top-level eligible items are narrowed to `floor` (if `Some`); inline `mod {}`
/// bodies are then recursed by `walk()`, so a tighter inline floor still applies
/// transitively. `crate_reexports` is consulted for every candidate (named match
/// OR glob sentinel).
///
/// # Skips
///
/// - `pub use` (re-exports are never narrowed).
/// - Items inside `impl` bodies (trait-impl method `pub` is irrelevant).
/// - `macro_rules!` definitions.
///
/// # Idempotency
///
/// Already-narrowed children have `Visibility::Restricted` and are skipped on a
/// second run.
///
/// # Allocation
///
/// The input is parsed directly with [`syn::parse_str`] - only the parsed
/// [`syn::File`] is needed, never the gap-anchored spans the reorder/lint model
/// computes. When no child needs narrowing (no restricted-visibility inline
/// module with bare-`pub` children, or an idempotent re-run), the input is
/// borrowed back unchanged ([`Cow::Borrowed`]) with zero allocation; otherwise
/// the rewritten buffer is returned as [`Cow::Owned`].
///
/// # Errors
///
/// Returns [`syn::Error`] when `source` is not valid Rust.
pub fn narrow_vis_in_tree<'a>(
    source: &'a str,
    floor: Option<&'a str>,
    crate_reexports: &ReexportSet,
) -> anyhow::Result<Cow<'a, str>> {
    let file: syn::File = syn::parse_str(source)?;
    let line_starts = line_start_offsets(source);
    let mut edits: Vec<(usize, usize, Cow<'_, str>)> = Vec::new();
    let names = crate_reexports.names();

    // Top-level items: narrow against the file's tree floor. With `floor = None`
    // (standalone, no crate context) this loop is skipped and only inline mods
    // narrow via `walk()`.
    if let Some(f) = floor {
        for item in &file.items {
            // Floor text is NOT a slice of `source` (it comes from the parent
            // declaration), so it is owned per edit (small, rare-path alloc).
            narrow_if_eligible_owned(item, f, &line_starts, Some(names), &mut edits);
        }
    }

    // Inline-mod recursion: tighter inline floors propagate; floor slices here
    // ARE into `source` (zero-alloc).
    walk(
        &file.items,
        floor,
        source,
        &line_starts,
        Some(names),
        &mut edits,
    );

    if edits.is_empty() {
        Ok(Cow::Borrowed(source))
    } else {
        Ok(Cow::Owned(apply_edits(source, edits)))
    }
}

/// Byte offset of the start of every line in `source` (line 1 starts at 0).
///
/// Built with a single SIMD-accelerated [`memchr`] scan, matching the sibling
/// `rust-llm-tidy-model` crate's approach.
pub(crate) fn line_start_offsets(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    // Heuristic preallocation. Capacity = bytes/D; no regrowth when the file's
    // average bytes/line >= D (the same D=21 the sibling model crate uses).
    let mut starts: Vec<usize> = Vec::with_capacity(bytes.len() / 21 + 1);
    starts.push(0);
    let mut from = 0;
    while let Some(pos) = memchr::memchr(b'\n', &bytes[from..]) {
        from += pos + 1;
        starts.push(from);
    }
    starts
}

/// Convert a 1-based line and 0-based byte column into a byte offset using the
/// precomputed `line_starts` table.
#[inline]
pub(crate) fn linecol_to_byte(line_starts: &[usize], line: usize, column: usize) -> usize {
    let idx = line.saturating_sub(1);
    let base = line_starts.get(idx).copied().unwrap_or(usize::MAX);
    base + column
}

/// Apply byte edits back-to-front (descending start offset) so earlier offsets
/// stay valid as later regions are rewritten.
///
/// The replacement text is the floor visibility, which is always at least as
/// long as the `pub` token it replaces (`pub` -> `pub(crate)` etc.), so the
/// output grows by a few bytes per edit. Capacity is preallocated with that
/// slack to keep `replace_range` from reallocating.
fn apply_edits(source: &str, mut edits: Vec<(usize, usize, Cow<'_, str>)>) -> String {
    edits.sort_by_key(|b| std::cmp::Reverse(b.0));
    let mut out = String::with_capacity(source.len() + edits.len() * 8);
    out.push_str(source);
    for (start, end, repl) in edits {
        out.replace_range(start..end, &repl);
    }
    out
}

/// Collect the simple names re-exported by any `pub use` (visibility `Public`)
/// found at any depth in `items` - top-level and inside inline modules. A glob
/// (`pub use p::*`) records the sentinel "*".
///
/// The set is allocated lazily via the `Option`: it stays `None` (no
/// allocation) for files with no `pub use`, which is the common case.
fn collect_reexports(items: &[syn::Item], out: &mut Option<AHashSet<String>>) {
    for item in items {
        if let syn::Item::Use(u) = item
            && matches!(u.vis, Visibility::Public(_))
        {
            let set = out.get_or_insert_with(AHashSet::new);
            collect_use_tree(&u.tree, set);
        }
        if let syn::Item::Mod(m) = item
            && let Some((_, content)) = &m.content
        {
            collect_reexports(content, out);
        }
    }
}

/// Same as `narrow_if_eligible` but the floor text is owned (tree path), so the
/// pushed edit wraps `Cow::Owned(floor.to_string())`. Kept as a thin twin to
/// preserve `walk()`'s hot path `&'src str` borrowing (no allocation).
fn narrow_if_eligible_owned(
    item: &syn::Item,
    floor: &str,
    line_starts: &[usize],
    reexported: Option<&AHashSet<String>>,
    edits: &mut Vec<(usize, usize, Cow<'_, str>)>,
) {
    let Some((vis, ident)) = eligible_vis_and_ident(item) else {
        return;
    };
    let Visibility::Public(_) = vis else {
        return;
    };
    if let Some(set) = reexported {
        let name = ident.to_string();
        if set.contains(&name) || set.contains("*") {
            return;
        }
    }
    let span = vis.span();
    let start = linecol_to_byte(line_starts, span.start().line, span.start().column);
    let end = linecol_to_byte(line_starts, span.end().line, span.end().column);
    edits.push((start, end, Cow::Owned(floor.to_string())));
}

/// Recursively walk items, descending only into inline `Item::Mod` bodies.
///
/// `floor` is the verbatim visibility text of the innermost restricted ancestor
/// module (e.g. `"pub(crate)"`), or `None` at the crate root / inside a
/// non-restricted ancestor. Children of a restricted module are narrowed in
/// place; the walk then recurses so the floor propagates transitively.
fn walk<'src>(
    items: &[syn::Item],
    floor: Option<&'src str>,
    source: &'src str,
    line_starts: &[usize],
    reexported: Option<&AHashSet<String>>,
    edits: &mut Vec<(usize, usize, Cow<'src, str>)>,
) {
    for item in items {
        let syn::Item::Mod(m) = item else {
            continue;
        };
        let Some((_, content)) = &m.content else {
            continue;
        };

        // A restricted-visibility inline module becomes the new floor; a `pub`
        // or private module inherits the enclosing floor (transitive).
        let new_floor = match &m.vis {
            Visibility::Restricted(_) => {
                let span = m.vis.span();
                let start = linecol_to_byte(line_starts, span.start().line, span.start().column);
                let end = linecol_to_byte(line_starts, span.end().line, span.end().column);
                Some(&source[start..end])
            }
            _ => floor,
        };

        for child in content {
            narrow_if_eligible(child, new_floor, line_starts, reexported, edits);
        }
        walk(content, new_floor, source, line_starts, reexported, edits);
    }
}

/// Expand a single `use` tree into its re-exported simple names.
fn collect_use_tree(tree: &syn::UseTree, out: &mut AHashSet<String>) {
    match tree {
        syn::UseTree::Path(p) => collect_use_tree(&p.tree, out),
        syn::UseTree::Name(n) => {
            out.insert(n.ident.to_string());
        }
        syn::UseTree::Rename(r) => {
            out.insert(r.rename.to_string());
        }
        syn::UseTree::Glob(_) => {
            out.insert(String::from("*"));
        }
        syn::UseTree::Group(g) => {
            for t in &g.items {
                collect_use_tree(t, out);
            }
        }
    }
}

/// If `item` is a bare-`pub` child under a restricted-visibility floor, push an
/// edit replacing the `pub` token with the floor's visibility text.
///
/// The replacement text borrows `source` (the floor is already a `&str` slice
/// of it), so no per-edit [`String`] is allocated. The re-export guard's name
/// is materialized lazily: only when a `pub use` exists AND the child is bare
/// `pub`, so the common path allocates nothing.
fn narrow_if_eligible<'src>(
    item: &syn::Item,
    floor: Option<&'src str>,
    line_starts: &[usize],
    reexported: Option<&AHashSet<String>>,
    edits: &mut Vec<(usize, usize, Cow<'src, str>)>,
) {
    let Some(floor) = floor else {
        return;
    };
    let Some((vis, ident)) = eligible_vis_and_ident(item) else {
        return;
    };

    // Only narrow a bare `pub`.
    let Visibility::Public(_) = vis else {
        return;
    };

    // Re-export guard: skip items whose name is re-exported via `pub use`. A
    // glob sentinel ("*") disables narrowing for every named child. The name
    // String is only built when a re-export set exists (rare path).
    if let Some(set) = reexported {
        let name = ident.to_string();
        if set.contains(name.as_str()) || set.contains("*") {
            return;
        }
    }

    let span = vis.span();
    let start = linecol_to_byte(line_starts, span.start().line, span.start().column);
    let end = linecol_to_byte(line_starts, span.end().line, span.end().column);
    edits.push((start, end, Cow::Borrowed(floor)));
}

/// Return the visibility reference and identifier for item kinds eligible for
/// narrowing (fn, struct, enum, union, type, const, static, mod, trait, extern
/// crate). Returns `None` for `use`, `impl`, `macro_rules!`, macro invocations,
/// and other kinds (those are never narrowed).
///
/// Returning a borrowed `&Visibility` and `&Ident` (rather than an owned name
/// [`String`]) lets the caller defer - and usually skip - the name allocation,
/// since the name is only needed by the rare re-export guard.
#[inline]
fn eligible_vis_and_ident(item: &syn::Item) -> Option<(&Visibility, &proc_macro2::Ident)> {
    match item {
        syn::Item::Fn(f) => Some((&f.vis, &f.sig.ident)),
        syn::Item::Struct(s) => Some((&s.vis, &s.ident)),
        syn::Item::Enum(e) => Some((&e.vis, &e.ident)),
        syn::Item::Union(u) => Some((&u.vis, &u.ident)),
        syn::Item::Type(t) => Some((&t.vis, &t.ident)),
        syn::Item::Const(c) => Some((&c.vis, &c.ident)),
        syn::Item::Static(s) => Some((&s.vis, &s.ident)),
        syn::Item::Mod(m) => Some((&m.vis, &m.ident)),
        syn::Item::Trait(t) => Some((&t.vis, &t.ident)),
        syn::Item::ExternCrate(e) => Some((&e.vis, &e.ident)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ReexportSet, collect_crate_reexports, narrow_vis_in_tree};
    use syn::parse_str as syn_parse;

    /// Force proc_macro2 fallback so span byte ranges are accurate.
    fn force() {
        proc_macro2::fallback::force();
    }

    fn parse(src: &str) -> syn::File {
        syn_parse(src).expect("test source must parse")
    }

    /// Standalone narrowing through the tree entry point: `floor = None` (no
    /// cross-file context) plus a per-file re-export guard built from the file
    /// itself. Exercises the same inline-mod path `walk()` takes; the unit tests
    /// below call it to cover inline narrowing, floors, and the re-export guard.
    fn narrow<'a>(src: &'a str) -> anyhow::Result<std::borrow::Cow<'a, str>> {
        let parsed: syn::File = syn_parse(src)?;
        let reexports = collect_crate_reexports(std::iter::once(&parsed));
        narrow_vis_in_tree(src, None, &reexports)
    }

    #[test]
    fn narrows_pub_inside_pub_crate_mod() {
        force();
        let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(crate) fn f"), "narrowed: {out}");
        assert!(!out.contains("pub fn f"), "bare pub gone: {out}");
    }

    #[test]
    fn narrows_struct_const_static() {
        force();
        let src = "pub(crate) mod m {\n    pub struct S;\n    pub const C: u32 = 0;\n    pub static G: u32 = 1;\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(crate) struct S"), "{out}");
        assert!(out.contains("pub(crate) const C"), "{out}");
        assert!(out.contains("pub(crate) static G"), "{out}");
    }

    #[test]
    fn leaves_pub_use_untouched() {
        force();
        let src = "pub(crate) mod m {\n    pub use crate::x;\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub use crate::x"), "pub use untouched: {out}");
    }

    #[test]
    fn narrows_pub_super_floor() {
        force();
        let src = "pub(super) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(super) fn f"), "{out}");
    }

    #[test]
    fn narrows_pub_in_path_floor() {
        force();
        let src = "pub(in crate::a) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(in crate::a) fn f"), "{out}");
    }

    #[test]
    fn nested_transitive_floor() {
        force();
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
        force();
        let src = "pub(crate) mod m {\n    macro_rules! mac {\n        () => {};\n    }\n}\n";
        let out = narrow(src).unwrap();
        assert_eq!(&*out, src, "macro_rules! must not be narrowed");
    }

    #[test]
    fn skips_impl_methods() {
        force();
        // struct narrowed; impl method `pub fn f` untouched (no descent into impl).
        let src = "pub(crate) mod m {\n    pub struct S;\n    impl S {\n        pub fn f() {}\n    }\n}\n";
        let out = narrow(src).unwrap();
        assert!(out.contains("pub(crate) struct S"), "{out}");
        assert!(out.contains("pub fn f"), "impl method pub untouched: {out}");
    }

    #[test]
    fn reexport_guard_skips_narrowing() {
        force();
        let src = "pub use m::f;\npub(crate) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(
            out.contains("pub fn f"),
            "re-exported item not narrowed: {out}"
        );
    }

    #[test]
    fn reexport_guard_group() {
        force();
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
        force();
        let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
        let once = narrow(src).unwrap();
        force();
        let twice = narrow(&once).unwrap();
        assert_eq!(once, twice, "narrow must be idempotent");
    }

    #[test]
    fn no_top_level_narrowing() {
        force();
        // Top-level pub fn is not narrowed (module visibility is out of scope).
        let src = "pub fn f() {}\n";
        let out = narrow(src).unwrap();
        assert_eq!(&*out, src, "top-level pub not narrowed");
    }

    #[test]
    fn clean_input_is_borrowed() {
        force();
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
        force();
        let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
        let out = narrow(src).unwrap();
        assert!(
            matches!(out, std::borrow::Cow::Owned(_)),
            "narrowing input must be owned, got borrowed"
        );
        assert!(out.contains("pub(crate) fn f"));
    }

    // --- T1: crate-wide re-export guard tests ---

    #[test]
    fn crate_reexports_union_across_files() {
        // File A re-exports `f`; file B re-exports `g` (group). Neither alone
        // would block both names; the crate-wide union must block f AND g.
        let a = parse("pub use crate::foo::f;\n");
        let b = parse("pub use crate::foo::{g, h};\n");
        let set = collect_crate_reexports([&a, &b]);
        assert!(set.blocks("f"), "f re-exported in file a");
        assert!(set.blocks("g"), "g re-exported in file b (group)");
        assert!(set.blocks("h"), "h re-exported in file b (group)");
        assert!(!set.blocks("x"), "x is not re-exported anywhere");
    }

    #[test]
    fn crate_reexports_glob_in_any_file_disables_crate_wide() {
        // A glob in ONE file sets the sentinel; every named child is blocked
        // crate-wide (conservative soundness default, Open Q6).
        let a = parse("pub fn untouched() {}\n");
        let b = parse("pub use crate::foo::*;\n");
        let set = collect_crate_reexports([&a, &b]);
        assert!(set.has_glob(), "glob sentinel set by file b");
        assert!(set.blocks("anything"), "glob blocks every name crate-wide");
    }

    #[test]
    fn crate_reexports_rename_uses_alias() {
        // `pub use foo::f as alias;` keys by the alias, matching the per-file guard.
        let a = parse("pub use crate::foo::f as alias;\n");
        let set = collect_crate_reexports([&a]);
        assert!(set.blocks("alias"), "rename keyed by alias");
        assert!(
            !set.blocks("f"),
            "original name is not the re-exported name"
        );
    }

    // --- T3: narrow_vis_in_tree tests ---

    #[test]
    fn narrow_vis_in_tree_narrows_top_level_with_floor() {
        force();
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
        force();
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
        force();
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
        force();
        let src = "pub fn f() {}\n";
        let reexports = ReexportSet::new();
        let once = narrow_vis_in_tree(src, Some("pub(crate)"), &reexports).unwrap();
        force();
        // Second pass: `pub(crate) fn f` is already restricted -> no edit.
        let twice = narrow_vis_in_tree(&once, Some("pub(crate)"), &reexports).unwrap();
        assert_eq!(&*once, &*twice, "narrow_vis_in_tree must be idempotent");
    }

    #[test]
    fn narrow_vis_in_tree_inline_mod_inherits_file_floor() {
        force();
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
        force();
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
