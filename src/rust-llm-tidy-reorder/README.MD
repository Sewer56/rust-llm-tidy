# rust-llm-tidy-reorder

> **⚠ Vibecoded jank - personal tooling, not production software.**
> 
> This project was built for my own LLM-assisted Rust workflows.
> It's quite frankly- shit compared to the polished diamonds I usually make.
> 
> It may have rough edges, including untested edge cases.
> Use at your own risk - or don't.

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-reorder.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-reorder/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Library for automatically reordering Rust source code. Built for LLM workflows.

## Features

- **Parse** - Extract top-level items (fn, struct, enum, impl, trait, use,
  const, static, mod, type, union) with span ranges and doc-comment pinning
- **Graph** - Walks the `syn` AST to collect reference edges and compute a
  10-phase ordering
- **Reorder** - Emit reordered source by byte-slicing the original, preserving
  preamble, trailer, and the source's dominant line endings (CRLF or LF)
- **Safety** - Verify line multiset equality between original and reordered
  output
- **I/O** - Atomic file writes via tempfile + rename

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rust-llm-tidy-reorder = "0.1.1"
```

## Usage

### Basic Example

Parse a source string, compute reorder, emit:

```rust
use rust_llm_tidy_model::parse::parse_source;
use rust_llm_tidy_reorder::graph::compute_order;
use rust_llm_tidy_reorder::reorder::Permutation;

let source = r#"
fn b() { a(); }
fn a() {}
"#;

let parsed = parse_source(source).unwrap();
let order = compute_order(&parsed).unwrap();
let perm = Permutation::new(parsed.items.len(), order).unwrap();
let output = rust_llm_tidy_reorder::reorder::emit(&parsed, &perm).unwrap();

assert!(output.contains("fn a()"));
assert!(output.contains("fn b()"));
```

### Advanced Example

Full pipeline with safety verification and atomic write:

```rust,no_run
use rust_llm_tidy_model::parse::parse_source;
use rust_llm_tidy_reorder::graph::compute_order;
use rust_llm_tidy_reorder::reorder::Permutation;
use rust_llm_tidy_model::safety::verify_line_preservation;
use rust_llm_tidy_model::io::atomic_write;
use std::path::Path;

let source = std::fs::read_to_string("src/main.rs")?;

let parsed = parse_source(&source)?;
let order = compute_order(&parsed)?;
let perm = Permutation::new(parsed.items.len(), order)?;
let output = rust_llm_tidy_reorder::reorder::emit(&parsed, &perm)?;

// Ensure no lines were lost or duplicated.
verify_line_preservation(&source, &output)?;

// Write atomically.
atomic_write(Path::new("src/main.rs"), &output)?;
# Ok::<(), anyhow::Error>(())
```

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
- Edges come from references in fields, bodies, types, supertraits, impl
  blocks, and macro bodies.
- Phase 4 is special: macro definitions come before their first use. A macro
  calling another local macro follows it. Top-level invocations of a
  locally-defined macro move right after their definition; external macros stay
  put.

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

- `#[cfg(test)] mod tests` always comes last, preserving original source order
  (stable).

- Macro definitions come before their first use.

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-llm-tidy-reorder
[Docs.rs]: https://docs.rs/rust-llm-tidy-reorder
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
