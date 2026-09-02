# rust-llm-tidy-lang

Per-language AST backends for the `rust-llm-tidy` tooling.

It holds the [`LanguageBackend`] contract - one language's tree-sitter
grammar, its producer into the shared item model, and the AST ops it
implements - plus the extension registry that resolves a backend per file
extension.

The Rust backend is a passthrough over the parse setup in
`rust-llm-tidy-model`; it adds no behavior of its own.

## Installation

```toml
[dependencies]
rust-llm-tidy-lang = "0.1.0"
```

## Usage

```rust
use rust_llm_tidy_lang::{LanguageBackend, RustBackend, backend_for};

let backend = backend_for("rs").expect("rs has a registered backend");
assert!(backend.ast_ops().contains(&"reorder"));

let parsed = RustBackend.parse("fn main() {}\n").unwrap();
assert_eq!(parsed.items.len(), 1);
```

## Features

- **Registry** - Resolve a file extension (ASCII case-insensitively) to its
  registered backend; extensions without one resolve no backend and can run
  no AST op.
- **Backends** - Each backend emits the model crate's
  [`ParseResult`]
  item shape, so reorder and lint passes consume one item model for every
  language.
- **Rust passthrough** - The `rs` backend reuses the model crate's
  tree-sitter-rust grammar and parse function unchanged.

## License

Licensed under [Apache 2.0].

[`LanguageBackend`]: https://docs.rs/rust-llm-tidy-lang/latest/rust_llm_tidy_lang/trait.LanguageBackend.html
[Apache 2.0]: ../../LICENSE
[`ParseResult`]: https://docs.rs/rust-llm-tidy-model/latest/rust_llm_tidy_model/parse/struct.ParseResult.html
