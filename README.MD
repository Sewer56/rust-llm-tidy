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

`rust-llm-tidy` is a single command that runs the full pipeline (fix,
reorder, vis, lints) by default on `.rs` and `.md` files:

```bash
# Tidy a file in place
rust-llm-tidy src/main.rs

# Preview changes without modifying the file
rust-llm-tidy --dry-run src/main.rs

# Tidy every .rs/.md file under a directory recursively
rust-llm-tidy src

# No paths given -> process the changed files in the current git diff
rust-llm-tidy

# Run only specific ops (repeatable); overrides the config `include` mode
rust-llm-tidy --include vis src/lib.rs
rust-llm-tidy --include reorder --include vis src

# Skip an op for this run (additive to config `exclude`)
rust-llm-tidy --exclude lints src

# Validate the config without processing files
rust-llm-tidy --validate
```

Global flags: `--dry-run`, `--config <PATH>`, `--no-config`, `--validate`,
`--include <OP>` (repeatable), `--exclude <OP>` (repeatable).

#### Operations

| Op | Does | Mutates | Default on? |
| -- | ---- | ------- | ----------- |
| `tables` | align GFM tables | yes | yes |
| `fences` | alternate nested fence delimiters | yes | yes |
| `links` | hoist repeated inline links | yes | yes |
| `reorder` | canonical item ordering | yes | yes |
| `vis` | narrow bare `pub` in restricted modules | yes | yes |
| `lints` | run DOC001-DOC006 + TEST001 checks | no | yes |

#### Feature documentation

| Feature | Page |
| ------- | ---- |
| Reorder | [docs/reorder.md](./docs/reorder.md) |
| Visibility narrowing | [docs/vis.md](./docs/vis.md) |
| Table alignment | [docs/tables.md](./docs/tables.md) |
| Fence delimiters | [docs/fences.md](./docs/fences.md) |
| Link hoisting | [docs/links.md](./docs/links.md) |
| Lints (DOC001-DOC006, TEST001) | [docs/lints.md](./docs/lints.md) |

### Configuration

`rust-llm-tidy` reads an optional YAML config file named
`.rust-llm-tidy.yml`. It supports three rule-selection keys:

- `include` - whitelist mode: for matched paths, run ONLY these rules.
  A file matching NO `include` group runs nothing.
- `exclude` - blacklist mode: for matched paths, never run these rules.
- `exclude_files` - skip ALL processing for matched files.

`include` and `exclude` are mutually exclusive (both non-empty -> config-load
error). Omitting `paths` in an `include` or `exclude` group implies `["**"]`
(all paths).

Additionally:

- `post_process` - external commands run on processed files after tidying.
  Each entry has `command`, `args` (default empty), and `extensions`
  (default empty = all). File path appended as last arg. Uses
  `std::process::Command` - no shell injection. Skipped under `--dry-run`
  and for excluded files.

Globs are relative to the config file's directory and use `literal_separator`
semantics (`*` does not cross `/`; `**` recurses). Rule names are the seven
lint codes (DOC001-DOC006, TEST001) plus the six ops: `tables`, `fences`,
`links`, `reorder`, `vis`, `lints`.

The config is discovered automatically: `rust-llm-tidy` walks up from the
current directory to the repo root (the first ancestor containing `.git`),
checking each level for `.rust-llm-tidy.yml`; the first found wins. Override
with `--config <PATH>`; disable all discovery and loading with `--no-config`.

Every config error - bad YAML, unknown rule name, bad glob syntax, or a pattern
matching zero files - produces a non-zero exit on every command. There is no
"warn and continue" mode. Use `--validate` to check the config without
processing files (useful in CI).

```bash
# Validate the config without processing files
rust-llm-tidy --validate

# Run with an explicit config path
rust-llm-tidy --config ./my-config.yml src

# Disable config discovery for an ad-hoc run
rust-llm-tidy --no-config --include tables README.md
```

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

See [docs/reorder.md](./docs/reorder.md) for an annotated 10-phase example.

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
- [rust-llm-tidy-vis]: Visibility narrowing - narrows bare `pub` items inside restricted-visibility inline modules to the module's visibility
- [rust-llm-tidy-lint]: Linter - missing-docs, incomplete-`# Errors`/`# Arguments`, placeholder, and test-naming checks
- [rust-llm-tidy-fix]: Markdown/doc-comment tidying - realigns GFM tables, fixes nested fence delimiters, and hoists repeated inline links in `.rs` doc comments and `.md` files
- [cli]: Unified command-line interface (fix, reorder, vis, lints)

## Git Hook (optional)

Auto-tidy staged `.rs` files on commit (fix + reorder + vis + lints + rustfmt). One-time setup per clone:

```bash
git config core.hooksPath .githooks
```

`pre-commit` is a dispatcher that runs every executable `*.sh` in
`.githooks/pre-commit.d/` in lexicographic order - name files with a number
prefix (`10-`, `20-`, ...) to fix the run order. Add hooks by dropping files
there. Works on Linux, macOS, and Windows (Git for Windows).

Bundled hooks, in run order:

- `10-tidy-rust.sh` - runs `rust-llm-tidy` on staged `.rs` files (fix,
  reorder, vis, lints), then `rustfmt` via the config `post_process` step. Uses
  an installed `rust-llm-tidy` binary if present, else `cargo run`.

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
[rust-llm-tidy-vis]: ./src/rust-llm-tidy-vis/README.MD
[rust-llm-tidy-lint]: ./src/rust-llm-tidy-lint/README.MD
[rust-llm-tidy-fix]: ./src/rust-llm-tidy-fix/README.MD
[cli]: ./src/cli/README.MD
[Developer Manual]: https://reloaded-project.github.io/reloaded-templates-rust/manual/
[Contributing Guide]: https://reloaded-project.github.io/reloaded-templates-rust/manual/#contributing
[Apache 2.0]: ./LICENSE
