# rust-llm-tidy-lint

The shared lint core for the `rust-llm-tidy` tooling: the `Diagnostic`
shape, the lint-code registry, and the TEXT001/TEXT002 text rules over
measured prose.

The AST item rules (DOC001-DOC006, TEST001) live with their languages:
each backend's `lints` module in `rust-llm-tidy-lang` implements the
codes over its own parse.

## Checks

| Code      | Severity | Fires when                                                             |
| --------- | -------- | ---------------------------------------------------------------------- |
| `DOC001`  | Error    | A non-private item has no `///` doc comment.                           |
| `DOC002`  | Error    | A `pub fn` returning `Result` has no `# Errors` section.               |
| `DOC003`  | Warning  | A `# Errors` section names no concrete error variant.                  |
| `DOC004`  | Warning  | A `pub fn` with parameters has no `# Arguments` section.               |
| `DOC005`  | Warning  | A `# Arguments` section does not mention every parameter name.         |
| `DOC006`  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`).        |
| `TEXT001` | Error    | A doc paragraph over 240 chars of full text.                           |
| `TEXT002` | Warning  | A doc line over 80 chars of full text.                                 |
| `TEST001` | Warning  | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |

The DOC and TEST codes are emitted by the `rust-llm-tidy-lang` backends;
the TEXT codes by this crate's text rules.

## Installation

```toml
[dependencies]
rust-llm-tidy-lint = "0.2.1"
```

## Usage

```rust
use rust_llm_tidy_lint::check;

let source = "A prose line that keeps going well past the eighty character line budget for docs.";
let diags = check::run_text_checks(source, "md");
assert!(diags.iter().any(|d| d.code == "TEXT002"));
```

The checks are pure functions over text or measured regions and produce
no side effects.

## License

Licensed under [Apache 2.0].

[Apache 2.0]: ../../LICENSE
