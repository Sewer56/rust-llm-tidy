# rust-llm-tidy-model

Shared language-neutral source model used by the `rust-llm-tidy-reorder` and
`rust-llm-tidy-lint` tooling.

Language backends produce top-level [`SourceItem`]s, each carrying:

- its byte span, kind, name, and visibility tier
- leading doc comments
- for functions: its parameter names, whether it returns `Result`, and
  whether it is a `#[test]` function

It also provides atomic file I/O and a line-multiset safety check.

This crate contains no reordering or checking logic of its own; it holds only
the shared source data. The `rust-llm-tidy-lang` crate owns language grammars
and parsing.

## Installation

```toml
[dependencies]
rust-llm-tidy-model = "0.3.1"
rust-llm-tidy-lang = "0.1.0"
```

## Usage

```rust
use rust_llm_tidy_lang::{LanguageBackend, RustBackend};

let source = r#"
pub fn main() {}
fn helper() {}
"#;

let parsed = RustBackend.parse(source).unwrap();
for item in &parsed.items {
    println!("{:?} {:?} vis={:?}", item.kind(), item.name(), item.visibility());
}
```

## Features

- **Parse results** - Store backend-produced items, source text, syntax
  trees, and preamble/trailer offsets.
- **Item metadata** - Store spans, visibility, documentation, function
  details, and in-type members.
- **Safety** - Verify line-multiset equality between original and transformed
  output.
- **Line endings** - Detect the source's dominant line ending (`\r\n` or
  `\n`) so transforms can preserve it on in-place writes.
- **I/O** - Atomic file writes via tempfile + rename.

## License

Licensed under [Apache 2.0].

[`SourceItem`]: https://docs.rs/rust-llm-tidy-model/latest/rust_llm_tidy_model/parse/struct.SourceItem.html
[Apache 2.0]: ../../LICENSE
