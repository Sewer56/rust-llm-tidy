# rust-doc-check

Documentation linter for Rust source files. Runs a set of documentation checks
over a parsed source file and reports missing docs and incomplete `# Errors`
sections.

## Checks

| Code      | Severity | Fires when                                                             |
| --------- | ------- | ---------------------------------------------------------------------- |
| `DOC001`  | Error   | A non-private item has no `///` doc comment.                           |
| `DOC002`  | Error   | A `pub fn` returning `Result` has no `# Errors` section.               |
| `DOC003`  | Warning | A `# Errors` section names no concrete error variant.                  |
| `DOC004`  | Warning | A `pub fn` with parameters has no `# Arguments` section.               |
| `DOC005`  | Warning | A `# Arguments` section does not mention every parameter name.         |
| `DOC006`  | Warning | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`/...).    |
| `TEST001` | Warning | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |

## Installation

```toml
[dependencies]
rust-doc-check = "0.1.0"
```

## Usage

```rust
use rust_doc_check::check;
use rust_source_model::parse;

let source = "pub fn load() -> Result<(), String> { Ok(()) }";
let parsed = parse::parse_source(source).unwrap();
let diags = check::run_all(&parsed);
assert!(!diags.is_empty());
```

The checks are pure functions over a `ParseResult` and produce no side effects.

## License

Licensed under [Apache 2.0].

[Apache 2.0]: ../../LICENSE
