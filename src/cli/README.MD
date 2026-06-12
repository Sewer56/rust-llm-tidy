# rust-auto-reorder-cli

[![Crates.io](https://img.shields.io/crates/v/rust-auto-reorder-cli.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-auto-reorder-cli/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-auto-reorder/actions/workflows/rust.yml/badge.svg)][CI]

Command-line interface for [rust-auto-reorder].

<!-- 
    This README is displayed on crates.io & docs.rs
    Write detailed documentation here for users.
-->

## Installation

```bash
cargo install rust-auto-reorder-cli
```

Or build from source:

```bash
cargo build --release -p rust-auto-reorder-cli
```

## Usage

The binary exposes three subcommands: `reorder` (reorder items), `check`
(read-only documentation linter), and `all` (reorder then check).

```bash
# Reorder a single file in place
rust-auto-reorder reorder src/main.rs

# Preview changes without modifying the file
rust-auto-reorder reorder --dry-run src/main.rs

# Recursively reorder every .rs file under a directory
rust-auto-reorder reorder src

# Preview recursive changes
rust-auto-reorder reorder --dry-run src

# Check documentation coverage (read-only, never writes)
rust-auto-reorder check src

# Reorder then check in one pass
rust-auto-reorder all src
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
always comes last. See the [rust-auto-reorder documentation](../rust-auto-reorder/README.MD)
for the full rules.

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-auto-reorder-cli
[Docs.rs]: https://docs.rs/rust-auto-reorder-cli
[CI]: https://github.com/Sewer56/rust-auto-reorder/actions
[rust-auto-reorder]: ../rust-auto-reorder/README.MD
