# rust-llm-tidy

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-reorder.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-reorder/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Rust project for automatically reordering Rust source file items into an
opinionated order with dependency resolution. Intended for LLM use.

> **⚠ Vibecoded jank - personal tooling, not production software.**
> 
> This project was built for my own LLM-assisted Rust workflows.
> It's quite frankly- shit compared to the polished diamonds I usually make.
> 
> It may have rough edges, including untested edge cases.
> Use at your own risk - or don't.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-llm-tidy-reorder = "0.1.0"
```

### CLI

The binary exposes three subcommands: `reorder` (the original behavior),
`check` (a read-only documentation linter), and `all` (reorder then check).

```bash
# Reorder a file in place
rust-llm-tidy reorder src/main.rs

# Preview changes without modifying the file
rust-llm-tidy reorder --dry-run src/main.rs

# Reorder every .rs file under a directory recursively
rust-llm-tidy reorder src

# Reorder multiple paths in one invocation (files and/or directories)
rust-llm-tidy reorder src/main.rs src/lib.rs tests

# Lint for missing docs, incomplete `# Errors`/`# Arguments` sections, placeholders, and test naming (read-only)
rust-llm-tidy check src

# Reorder then check in one pass
rust-llm-tidy all src
```

#### `check` rules

`check` reports the following diagnostics. It never writes files and exits
non-zero when any error-severity finding is present (warnings do not fail).

| Code      | Severity | Fires when                                                             |
| --------- | -------- | ---------------------------------------------------------------------- |
| `DOC001`  | Error    | A non-private item has no `///` doc comment.                           |
| `DOC002`  | Error    | A `pub fn` returning `Result` has no `# Errors` section.               |
| `DOC003`  | Warning  | A `# Errors` section names no concrete error variant.                  |
| `DOC004`  | Warning  | A `pub fn` with parameters has no `# Arguments` section.               |
| `DOC005`  | Warning  | A `# Arguments` section does not mention every parameter name.         |
| `DOC006`  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`/...).    |
| `TEST001` | Warning  | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |

`DOC001` covers non-private functions, structs, enums, unions, type aliases,
traits, consts, and statics. Modules are skipped: they are documented via `//!`
inner docs that may live in a separate file this single-file checker does not
parse.

### Programmatic

```rust
use rust_llm_tidy_model::parse;
use rust_llm_tidy_reorder::graph;
use rust_llm_tidy_reorder::reorder::Permutation;

let source = r#"
fn b() { a(); }
fn a() {}
"#;

let parsed = parse::parse_source(source).unwrap();
let order = graph::compute_order(&parsed).unwrap();
let perm = Permutation::new(parsed.items.len(), order).unwrap();
let output = rust_llm_tidy_reorder::reorder::emit(&parsed, &perm).unwrap();

println!("{}", output);
// fn a() {}
// fn b() { a(); }
```

See the [rust-llm-tidy crate documentation] for detailed usage.

## Ordering Rules

The algorithm orders top-level items across phases. Within most phases, an item
comes before any item it references; alphabetical order breaks ties.

Below is an annotated example of the output order. The comments show which
phase each item belongs to and why.

```rust,ignore
// Phase 1: extern crate and uncategorized items (items not matching any other phase) stay in original order.
extern crate serde;

// Phase 2: use declarations.
use std::collections::HashMap;
use std::fs;

// Phase 3: mod declarations sort alphabetically by name.
mod alpha;
mod beta;

// Phase 4: Macros come before their first use. A top-level invocation of a
// locally-defined macro follows its definition (Rust macro_rules! use textual
// scoping). External macros (no local def) stay in phase 1.
macro_rules! say_hello { () => {}; }
say_hello!();

// Phase 5: const and static items (referrers before referenced items).
const MAX: usize = 100;
static COUNT: AtomicUsize = AtomicUsize::new(0);

// Phase 6: Type items (referrers before referenced items). App references
// Config, so App comes first.
struct App { config: Config }
struct Config { value: i32 }

// Phase 7: Trait items.
trait Drawable { fn draw(&self); }

// Phase 8: impl blocks follow their type. Inherent impls come before trait
// impls for the same type. Orphan impls (impls for a type not defined in this
// file) come last.
impl App { fn new() -> Self { todo!() } }
impl Drawable for App { fn draw(&self) { todo!() } }

// Phase 9: Free functions by visibility tier: pub -> restricted -> private.
// Within each tier, main comes first, callers precede callees, and alphabetical
// order breaks ties.
pub fn public_fn() { private_helper(); }
pub(crate) fn crate_fn() {}
fn main() {}
fn private_helper() {}

// Phase 10: #[cfg(test)] mod tests always comes last.
#[cfg(test)]
mod tests {}
```

### Dependencies

Phases 5-7 and 9 put referrers before the items they reference.

- If `B` references `A` by name, `B` appears before `A`.
- Self-references are ignored.
- Cycles stay contiguous and sort alphabetically.
- Edges come from references in fields, bodies, types, supertraits, impl blocks, and macro bodies.
- Phase 4 is special: macro definitions come before their first use. A macro calling another local macro follows it. Top-level invocations of a locally-defined macro move right after their definition; external macros stay put.

### Function visibility grouping (Phase 9)

Functions split into three visibility tiers:

- Tier 1: `pub` - fully public functions.

- Tier 2: `pub(crate)`, `pub(super)`, `pub(in path)` - restricted visibility.

- Tier 3: private - no visibility modifier.

Within each tier, `main` comes first, callers precede callees, and alphabetical
order breaks ties.

### `use` handling

`use` declarations are emitted as a contiguous block in their original source
order - this tool does not sort imports. Use rustfmt (with `reorder_imports`)
for sorting, grouping, and merging.

### Special cases

- `fn main()` always comes first in its visibility tier.

- `#[cfg(test)] mod tests` always comes last, preserving original source order (stable).

- Macro definitions come before their first use.

## Crates

- [rust-llm-tidy-model]: Shared source model - parse top-level items with spans, visibility, and doc comments; atomic I/O; line-multiset safety
- [rust-llm-tidy-reorder]: Reordering library - reference-graph ordering of items into canonical 10-phase order
- [rust-llm-tidy-lint]: Linter - missing-docs, incomplete-`# Errors`/`# Arguments`, placeholder, and test-naming checks
- [cli]: Unified command-line interface (`reorder`, `check`, `all`)

## Git Hook (optional)

Auto-reorder and format staged `.rs` files on commit. One-time setup per clone:

```bash
git config core.hooksPath .githooks
```

`pre-commit` is a dispatcher that runs every executable `*.sh` in
`.githooks/pre-commit.d/` in lexicographic order - name files with a number
prefix (`10-`, `20-`, ...) to fix the run order. Add hooks by dropping files
there. Works on Linux, macOS, and Windows (Git for Windows).

Bundled hooks, in run order:

- `10-reorder-rust.sh` - reorders staged `.rs` files. Uses an installed
  `rust-llm-tidy` binary if present, else `cargo run`.
- `20-rustfmt.sh` - runs `rustfmt --edition 2021` on staged `.rs` files so
  import sorting/grouping (handled by rustfmt) and general formatting apply
  after the reorder step.

## Developer Manual

For step-by-step development guidance, see the [Developer Manual].

## Contributing

We welcome contributions! See the [Contributing Guide] for details.

## License

Licensed under [Apache 2.0].

[Crates.io]: https://crates.io/crates/rust-llm-tidy-reorder
[Docs.rs]: https://docs.rs/rust-llm-tidy-reorder
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[rust-llm-tidy crate documentation]: ./src/rust-llm-tidy-reorder/README.MD
[rust-llm-tidy-model]: ./src/rust-llm-tidy-model/README.MD
[rust-llm-tidy-reorder]: ./src/rust-llm-tidy-reorder/README.MD
[rust-llm-tidy-lint]: ./src/rust-llm-tidy-lint/README.MD
[cli]: ./src/cli/README.MD
[Developer Manual]: https://reloaded-project.github.io/reloaded-templates-rust/manual/
[Contributing Guide]: https://reloaded-project.github.io/reloaded-templates-rust/manual/#contributing
[Apache 2.0]: ./LICENSE
