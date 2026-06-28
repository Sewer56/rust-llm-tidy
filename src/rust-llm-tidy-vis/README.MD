# rust-llm-tidy-vis

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-vis.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-vis/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Narrows bare `pub` items nested inside restricted-visibility inline modules to
the enclosing module's visibility. Built for LLM-assisted Rust workflows.

## The rule

A `pub` item inside a `pub(crate)` module is effectively `pub(crate)` - the bare
`pub` keyword overstates the item's reachability. [`narrow_vis_in_tree`] rewrites
the `pub` token of each affected child to the most-restrictive enclosing module's
visibility:

| Enclosing module | Child `pub` becomes |
| ---------------- | ------------------- |
| `pub(crate)`     | `pub(crate)`        |
| `pub(super)`     | `pub(super)`        |
| `pub(in path)`   | `pub(in path)`      |

Edits are byte-precise on the `pub` token, so doc-comments and attributes above
the item are never shifted.

## Scope (crate-aware)

The CLI narrows bare `pub` items using **cross-file** module visibility: a
top-level `pub fn f()` in `foo.rs` whose parent declares `pub(crate) mod foo;`
in `lib.rs` is narrowed to `pub(crate)`, even though the restriction lives in a
different file.

Crate-root discovery runs `cargo metadata --no-deps` (adds ~50-200ms and needs
`cargo` on `PATH` with a valid `Cargo.toml` at or above the input). If no
`Cargo.toml` is found or `cargo metadata` fails, the CLI warns and narrows each
file standalone (no cross-file floor; inline `mod {}` still narrows), so
standalone files keep working unchanged.

The library exposes one entry point:

- [`narrow_vis_in_tree`] - crate-aware (a file's effective floor from a
  [`ModuleTree`] plus a crate-wide [`ReexportSet`]). Pass `floor = None` and a
  per-file [`ReexportSet`] for standalone narrowing.

## Soundness limits

Cross-file mode is the default, so these limits ship to every user. The
crate-wide re-export guard (`pub use` scanned across every file) is a hard
correctness gate. These remain impossible or unsafe even with cross-file mode:

- **Cross-crate reachability**: a downstream crate can `pub use` an item, keeping
  it effectively `pub`. Cannot be detected without compiling dependencies.
- **Macro-generated `mod` decls**: invisible to the source-tree scan
  (proc-macro / `macro_rules!` expansion is not visible to tree-sitter).
- **`#[cfg]`-gated modules**: treated as present (conservative).
- **Re-export completeness**: the crate-wide guard covers same-crate `pub use`
  only; cross-crate re-exports also defeat narrowing.
- **Multiple restricted ancestors**: the floor uses the innermost restricted
  `mod`'s visibility (matches the inline rule); incomparable visibilities
  (`pub(crate)` vs `pub(in path)`) are not ranked.

## Diagnostics

- ``warning: <file>: `mod foo;` resolves to no `foo.rs` or `foo/mod.rs` `` - unresolved
  module declaration.
- `warning: <file>: #[path = "..."] target not found` - missing `#[path]` target.

## Re-export guard

An item re-exported at wider visibility (`pub use m::f;`, `pub use m::{f, g};`,
`pub use m::f as alias;`) is NOT narrowed - the re-export widens its effective
reachability beyond the module floor.

The guard scans every `pub use` in the file (top-level and nested in modules)
and expands groups, renames (using the alias), and globs. A glob (`pub use m::*`)
records an unknown set, so narrowing is disabled for every named child
(conservative: "if unsure, leave it alone").

## What is skipped

- `pub use` re-exports (never narrowed).
- Items inside `impl` bodies (trait-impl method `pub` is irrelevant to
  reachability and to trait contracts).
- `macro_rules!` definitions.

## Idempotency

Already-narrowed children have restricted visibility and are skipped on a second
run, so `narrow_vis_in_tree(narrow_vis_in_tree(src, ..), ..)` is a no-op.

## No-op cost

[`narrow_vis_in_tree`] parses the input directly and returns a
[`std::borrow::Cow<str>`] - borrowed back unchanged with zero allocation when
nothing needs narrowing, or owned when items are rewritten. (It needs only the
parsed syntax tree, so it skips the per-item span work the reorder pass does.)

## Usage

```rust
use rust_llm_tidy_vis::{narrow_vis_in_tree, ReexportSet};

let src = "pub(crate) mod m {\n    pub fn f() {}\n}\n";
let reexports = ReexportSet::new();
let out = narrow_vis_in_tree(src, None, &reexports).unwrap();
assert!(out.contains("pub(crate) fn f"));
```

### Crate-aware

```rust,ignore
use rust_llm_tidy_vis::{
    ParsedFile, build_module_tree, collect_crate_reexports, discover_crate_root,
    narrow_vis_in_tree,
};
use std::path::PathBuf;

let root = discover_crate_root(std::path::Path::new("src/lib.rs")).unwrap();
// (path, source) for every .rs in the crate.
let sources: Vec<(PathBuf, String)> = vec![/* ... */];
// Parse each file once; the parsed files feed both the module tree and the
// crate-wide re-export scan (single parse per file).
let files: Vec<ParsedFile> = sources
    .into_iter()
    .filter_map(|(p, s)| ParsedFile::new(p, s).ok())
    .collect();
let tree = build_module_tree(&root, &files).unwrap();
let reexports = collect_crate_reexports(&files);
let path = PathBuf::from("src/foo.rs");
let out = narrow_vis_in_tree("pub fn f() {}\n", tree.floor_for(&path), &reexports).unwrap();
```

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-llm-tidy-vis
[Docs.rs]: https://docs.rs/rust-llm-tidy-vis
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[`narrow_vis_in_tree`]: https://docs.rs/rust-llm-tidy-vis/latest/rust_llm_tidy_vis/fn.narrow_vis_in_tree.html
[`ModuleTree`]: https://docs.rs/rust-llm-tidy-vis/latest/rust_llm_tidy_vis/struct.ModuleTree.html
[`ReexportSet`]: https://docs.rs/rust-llm-tidy-vis/latest/rust_llm_tidy_vis/struct.ReexportSet.html
[`std::borrow::Cow<str>`]: https://doc.rust-lang.org/std/borrow/enum.Cow.html
