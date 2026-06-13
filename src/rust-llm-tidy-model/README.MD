# rust-llm-tidy-model

Shared Rust source model used by the `rust-llm-tidy-reorder` and `rust-llm-tidy-lint`
tooling. It parses a Rust source file with [`syn`] and exposes a flat list of
top-level [`SourceItem`]s, each carrying its byte span, kind, name, visibility
tier, leading doc comments, and (for functions) its parameter names, whether it
returns `Result`, and whether it is a `#[test]` function. It also provides
atomic file I/O and a line-multiset safety check.

This crate contains no reordering or checking logic of its own; it holds only
the shared parsing primitives.

## Installation

```toml
[dependencies]
rust-llm-tidy-model = "0.1.0"
```

## Usage

```rust
use rust_llm_tidy_model::parse;

let source = r#"
pub fn main() {}
fn helper() {}
"#;

let parsed = parse::parse_source(source).unwrap();
for item in &parsed.items {
    println!("{:?} {:?} vis={:?}", item.kind(), item.name(), item.visibility());
}
```

## Features

- **Parse** - Extract top-level items (fn, struct, enum, impl, trait, use,
  const, static, mod, type, union) with span ranges and comment pinning.
- **Visibility** - Classify every named item as `pub`, `pub(crate)`/restricted,
  or private.
- **Docs** - Capture each item's leading `///` doc comments.
- **Function details** - For functions: named parameters (excluding the
  receiver), whether the return type is `Result`, and whether the function
  carries a `#[test]` attribute.
- **Safety** - Verify line-multiset equality between original and transformed
  output.
- **I/O** - Atomic file writes via tempfile + rename.

## License

Licensed under [Apache 2.0].

[`syn`]: https://docs.rs/syn
[`SourceItem`]: https://docs.rs/rust-llm-tidy-model/latest/rust_llm_tidy_model/parse/struct.SourceItem.html
[Apache 2.0]: ../../LICENSE
