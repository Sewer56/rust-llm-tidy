# rust-llm-tidy-reorder

> **⚠ Vibecoded jank - personal tooling, not production software.**
> 
> Built for my own LLM-assisted Rust workflows.
>
> It's quite frankly- shit compared to the polished diamonds I usually make.
>
> Use at your own risk - or don't.

[![Crates.io][crates-badge]][Crates.io]
[![Docs.rs][docs-badge]][Docs.rs]
[![CI][ci-badge]][CI]

Library for automatically reordering Rust source code. Built for LLM workflows.

## Features

- **Parse** - Extract top-level items (fn, struct, enum, impl, trait, use,
  const, static, mod, type, union) with span ranges and doc-comment pinning
- **Graph** - Walks the parse tree to collect reference edges and compute
  the profile-supplied ordering
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
rust-llm-tidy-lang = "0.1.0"
rust-llm-tidy-model = "0.3.1"
```

## Usage

### Basic Example

Parse a source string, compute reorder, emit:

```rust
use rust_llm_tidy_lang::{LanguageBackend, RustBackend};
use rust_llm_tidy_reorder::reorder::emit;

let source = r#"
fn b() { a(); }
fn a() {}
"#;

let parsed = RustBackend.parse(source).unwrap();
let perm = RustBackend
    .reorder_permutation(&parsed)
    .unwrap()
    .unwrap(); // the Rust backend always reorders
let output = emit(&parsed, &perm).unwrap();

assert!(output.contains("fn a()"));
assert!(output.contains("fn b()"));
```

### Advanced Example

Full pipeline with safety verification and atomic write:

```rust,no_run
use rust_llm_tidy_lang::{LanguageBackend, RustBackend};
use rust_llm_tidy_model::safety::verify_line_preservation;
use rust_llm_tidy_model::io::atomic_write;
use rust_llm_tidy_reorder::reorder::emit;
use std::path::Path;

let source = std::fs::read_to_string("src/main.rs")?;

let parsed = RustBackend.parse(&source)?;
let perm = RustBackend
    .reorder_permutation(&parsed)?
    .expect("the Rust backend always reorders");
let output = emit(&parsed, &perm)?;

// Ensure no lines were lost or duplicated.
verify_line_preservation(&source, &output)?;

// Write atomically.
atomic_write(Path::new("src/main.rs"), &output)?;
# Ok::<(), anyhow::Error>(())
```

## Ordering Rules

The algorithm orders top-level items across phases. Within most phases, an item
comes before any item it references; alphabetical order breaks ties.

Ordering is defined per language. [`compute_order`] takes a [`ReorderProfile`]
that supplies the phases, their ordering strategies, and the node kinds the
reference walk matches.

- The `rust-llm-tidy-lang` crate's Rust backend supplies the Rust order
  documented here through its `reorder_permutation`.
- The same crate's C# backend ships `CSharpProfile`; its rules are documented
  in `docs/reorder/csharp.md`.

Items reorder only within their preprocessor region (see `SourceItem::region`).
Items with in-type members can also reorder those members through
[`compute_member_order`].

Below is an annotated example of the output order. The comments show which
phase each item belongs to and why.

```rust,ignore
// Phase 1: extern crate and uncategorized items (items not matching any other
// phase) stay in original order.
extern crate serde;

// Phase 2: use declarations.
use std::collections::HashMap;
use std::fs;

// Phase 3: mod x; declarations sort alphabetically via rustfmt. Only
// inline #[cfg(test)] mod { ... } definitions move to phase 10.
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

// Phase 10: Inline #[cfg(test)] mod { ... } last (stable). Other mods
// stay in phase 3.
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

- Inline `#[cfg(test)] mod { ... }` definitions last, preserving source
  order (stable). File-based `mod x;` stays in phase 3.

- Macro definitions come before their first use.

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-llm-tidy-reorder
[Docs.rs]: https://docs.rs/rust-llm-tidy-reorder
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[crates-badge]: https://img.shields.io/crates/v/rust-llm-tidy-reorder.svg
[docs-badge]: https://docs.rs/rust-llm-tidy-reorder/badge.svg
[ci-badge]: https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg
[`compute_order`]: crate::graph::compute_order
[`compute_member_order`]: crate::graph::compute_member_order
[`ReorderProfile`]: crate::graph::ReorderProfile
