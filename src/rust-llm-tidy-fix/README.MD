# rust-llm-tidy-fix

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-fix.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-fix/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Realigns misaligned GitHub-Flavored Markdown (GFM) tables, rewrites nested
fence markers to alternate backticks and tildes, and hoists repeated inline
links to reference definitions. Pure text processing, zero dependencies.

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

## Nested fence delimiter safety

[`fix_fences`] flips each nested fence to the opposite marker (backticks <->
tildes) so an inner fence can't close the outer block early.

Applies to `.md` files and to fences inside `///` and `//!` doc comments.

Before (both backticks - inner closes outer early):

````text
```text
```rust
inner
```
````

After (inner flipped to tildes):

````text
```text
~~~rust
inner
~~~
````

## Repeated inline-link hoisting

[`fix_links`] rewrites inline links `[text](url)` that appear 2+ times in the
same document to reference-style `[text]` in prose plus a single trailing
`[text]: url` definition, reducing token count and visual clutter.

Before (the same pair is repeated verbatim):

```text
See [the docs](https://example.com) for setup and
[the docs](https://example.com) for troubleshooting.
```

After (hoisted to reference style):

```text
See [the docs] for setup and
[the docs] for troubleshooting.

[the docs]: https://example.com
```

The pass skips links that would not benefit or would break:

- **Single-occurrence** inline links (no repetition to hoist).
- **Autolink** (`<url>`) or whitespace/newline-URL forms.
- Already **reference-style** links (`[text][ref]`, `[text][]`, `[text]`).
- Links whose **text** already has a `[text]:` definition.
- Links inside **fenced code blocks** (reuses the same fence-tracking
  discipline as `fix_fences`).

The function is idempotent: an already-hoisted document is returned unchanged.
The hoisted `[text]: url` definitions use the source's dominant line ending
(`\r\n` or `\n`), so a CRLF document stays CRLF after hoisting.

## Usage

```rust
use rust_llm_tidy_fix::{fix_tables, fix_links};

let input = " | a   | bb |
| ------------- | --- |
| ccc           | d   |
";
let out = fix_tables(input);
assert_ne!(&*out, input);

// Link hoisting works independently:
let links = "see [A](http://x) and [A](http://x)\n";
let out = fix_links(links);
assert_eq!(&*out, "see [A] and [A]\n[A]: http://x\n");
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
