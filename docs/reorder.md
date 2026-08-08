# `reorder` - canonical item ordering

## What it does

Reorders top-level items of a Rust source file into a canonical 10-phase
order. Within most phases an item precedes anything it references;
alphabetical order breaks ties. `fn main()` comes first in its tier;
`#[cfg(test)] mod tests` comes last.

## Phases

```rust,ignore
// 1. extern crate + uncategorized items keep source order.
extern crate serde;

// 2. use declarations (not sorted by this tool - use rustfmt).
use std::fs;

// 3. mod declarations sort alphabetically.
mod alpha;

// 4. Macros come before their first use; top-level invocations of local
//    macros move right after their definition.
macro_rules! say_hello { () => {}; }
say_hello!();

// 5-7. const/static, type items, traits (referrers before referenced).
const MAX: usize = 100;
struct App { config: Config }
struct Config { value: i32 }
trait Drawable { fn draw(&self); }

// 8. impls follow their type; inherent impls before trait impls; orphans last.
impl App { fn new() -> Self { todo!() } }
impl Drawable for App { fn draw(&self) { todo!() } }

// 9. Free functions by visibility tier; callers before callees.
pub fn public_fn() { private_helper(); }
fn main() {}
fn private_helper() {}

// 10. #[cfg(test)] mod tests always last.
#[cfg(test)]
mod tests {}
```

## Before

```rust,ignore
fn a() {}

fn b() { a(); }
```

## After

```rust,ignore
fn b() { a(); }
fn a() {}
```

## Config

Scope reorder via the config or flags:

```yaml
# Whitelist: run only reorder on src/**/*.rs
include:
  - paths: ["src/**/*.rs"]
    rules: [reorder]

# Blacklist: never reorder generated code
exclude:
  - paths: ["**/generated/**"]
    rules: [reorder]
```

```bash
# CLI: run only reorder for this invocation
rust-llm-tidy --include reorder --dry-run src/lib.rs
# CLI: skip reorder for this invocation
rust-llm-tidy --exclude reorder src
```

## Dry-run output

`--dry-run` reports each would-be move instead of modifying the file. In text
mode the record prints to stderr:

```text
src/lib.rs:20: success[REORDER]: rearrange fn a_main from pos 2 to pos 1 (before b_helper) (fn `a_main`)
```

In JSON mode the same record appears on stdout with `severity: "success"` and
the reorder extras `from`/`to`/`before_name`:

```json
[
  {
    "path": "src/lib.rs",
    "line": 20,
    "severity": "success",
    "code": "REORDER",
    "message": "rearrange fn a_main from pos 2 to pos 1 (before b_helper)",
    "item_kind": "fn",
    "item_name": "a_main",
    "from": 2,
    "to": 1,
    "before_name": "b_helper"
  }
]
```

See [Dry-run change reporting](./lints.md#dry-run-change-reporting) for the
shared format.
