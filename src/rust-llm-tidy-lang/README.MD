# rust-llm-tidy-lang

Per-language AST backends for the `rust-llm-tidy` tooling.

Each language implements the [`LanguageBackend`] trait: its tree-sitter
grammar, its parse into the shared item model, and its AST ops. The
extension registry maps a file extension to its backend.

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

- **Registry** - Maps a file extension (ASCII case-insensitive) to its
  backend; unregistered extensions get none and run no AST ops.
- **Backends** - All emit the model crate's [`ParseResult`] shape, so
  reorder and lint passes share one item model.
- **Parsing** - Each backend owns its language's tree-sitter grammar,
  item classification, comment-pinned span extraction, and ordering
  policy.
- **Visibility narrowing** - Narrows fully public items inside
  restricted-visibility modules to the module's visibility; re-exported
  names stay public.

## Supported languages

- Rust (`rs`) - reorder, visibility narrowing, lints
- C# (`cs`) - reorder, lints
- Python (`py`, `pyi`) - no AST ops; the parse feeds the text checks

## License

Licensed under [Apache 2.0].

[`LanguageBackend`]: https://docs.rs/rust-llm-tidy-lang/latest/rust_llm_tidy_lang/trait.LanguageBackend.html
[Apache 2.0]: ../../LICENSE
[`ParseResult`]: https://docs.rs/rust-llm-tidy-model/latest/rust_llm_tidy_model/parse/struct.ParseResult.html
