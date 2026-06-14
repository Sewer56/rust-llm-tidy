# rust-llm-tidy-fix

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-fix.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-fix/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Realigns misaligned GitHub-Flavored Markdown (GFM) tables. Pure text
processing, zero dependencies.

## What it does

Given text containing a GFM table whose pipe borders and padding no longer
line up, [`fix_tables`] re-pads every column to its widest cell so the borders
align:

| Name     | Value |
| -------- | ----- |
| a        | 1     |
| longname | 200   |

It works on:

- Plain markdown (`.md`): tables are realigned directly.
- Rust doc comments (`.rs`): a leading `///` or `//!` prefix is stripped, the
  table is realigned, and the prefix is re-applied.

The function is idempotent: an already-aligned table is returned unchanged.

## Usage

```rust
use rust_llm_tidy_fix::fix_tables;

let input = "| a | bb |
| --- | --- |
| ccc | d |
";
let out = fix_tables(input);
assert_ne!(&*out, input);
```

## Alignment

Per-column alignment is parsed from the delimiter row:

| Delimiter | Alignment | Padding |
| --------- | --------- | ------- |
| `---`     | default   | right   |
| `:---`    | left      | right   |
| `:---:`   | center    | split   |
| `---:`    | right     | left    |

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-llm-tidy-fix
[Docs.rs]: https://docs.rs/rust-llm-tidy-fix
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
