# rust-llm-tidy-cli

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-cli.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-cli/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Command-line interface for [rust-llm-tidy-reorder].

<!-- 
    This README is displayed on crates.io & docs.rs
    Write detailed documentation here for users.
-->

## Installation

```bash
cargo install rust-llm-tidy-cli
```

Or build from source:

```bash
cargo build --release -p rust-llm-tidy-cli
```

## Usage

The binary exposes four subcommands: `fix` (realign GitHub-Flavored Markdown (GFM) tables and fix nested fence delimiters),
`reorder` (reorder items), `check` (read-only documentation linter), and `all`
(fix, reorder, then check).

```bash
# Reorder a single file in place
rust-llm-tidy reorder src/main.rs

# Preview changes without modifying the file
rust-llm-tidy reorder --dry-run src/main.rs

# Recursively reorder every .rs file under a directory
rust-llm-tidy reorder src

# Preview recursive changes
rust-llm-tidy reorder --dry-run src
```

```bash
# Fix (realign GFM tables and fix nested fence delimiters) in .rs doc comments and .md files
rust-llm-tidy fix README.md

# Preview fixes without modifying files
rust-llm-tidy fix --dry-run src

# Recursively fix every .rs and .md file under a directory
rust-llm-tidy fix docs
```

```bash
# Check documentation coverage (read-only, never writes)
rust-llm-tidy check src

# Fix, reorder, then check in one pass
rust-llm-tidy all src
```

## Ordering Rules

The algorithm orders top-level items across phases. Within most phases, an item
comes before any item it references; alphabetical order breaks ties. An annotated example:

```rust,ignore
// 1. extern crate and uncategorized items (items not matching any other phase) stay in original order.
extern crate serde;

// 2. use declarations.
use std::collections::HashMap;
use std::fs;

// 3. mod declarations sort alphabetically by name.
mod alpha;
mod beta;

// 4. Macros come before their first use.
macro_rules! say_hello { () => {}; }

// 5-7. const/static, type items, and traits (referrers before referenced items).
const MAX: usize = 100;
struct App { config: Config }
struct Config { value: i32 }
trait Drawable {}

// 8. impl blocks follow their type; inherent impls come before trait impls.
impl App {}
impl Drawable for App {}

// 9. Free functions by visibility tier: pub -> restricted -> private.
//    Within each tier, main comes first and callers precede callees.
pub fn public_fn() {}
fn main() {}
fn private_helper() {}

// 10. #[cfg(test)] mod tests always comes last.
#[cfg(test)]
mod tests {}
```

`fn main()` always comes first in its visibility tier. `#[cfg(test)] mod tests`
always comes last. See the [rust-llm-tidy-reorder documentation](../rust-llm-tidy-reorder/README.MD)
for the full rules.

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-llm-tidy-cli
[Docs.rs]: https://docs.rs/rust-llm-tidy-cli
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[rust-llm-tidy-reorder]: ../rust-llm-tidy-reorder/README.MD
