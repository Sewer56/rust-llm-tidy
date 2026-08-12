# rust-llm-tidy-fix

[![Crates.io]][crates-io]
[![Docs.rs]][docs-rs]
[![CI]][ci-actions]

Realigns misaligned GitHub-Flavored Markdown (GFM) tables, rewrites nested
fence markers to alternate backticks and tildes, and hoists inline links to
reference definitions. Pure text processing with no Markdown parser dependency.

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

[`fix_links`] rewrites every eligible inline link `[text](url)` to reference
style `[text]` plus a `[text]: url` definition. The hoist threshold defaults to
1, tunable via `links.min_occurrences`.

Definitions land inside each doc comment that uses the label (`.rs`), or in one
trailing block at the end of the document (`.md`).

Markdown - before:

```text
See [the docs](https://example.com) for setup.
```

After:

```text
See [the docs] for setup.

[the docs]: https://example.com
```

Rust doc comment - before:

```text
/// See [the docs](https://example.com) for setup.
```

After:

```text
/// See [the docs] for setup.
/// [the docs]: https://example.com
```

Never hoisted: autolinks and whitespace-URL forms, already-reference-style
links, texts with an existing definition, and links inside fenced code blocks
(same fence discipline as `fix_fences`).

Idempotent: already-hoisted output returns unchanged. Definitions use the
source's dominant line ending (`\r\n` or `\n`), so CRLF stays CRLF.

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

[Crates.io]: https://img.shields.io/crates/v/rust-llm-tidy-fix.svg
[Docs.rs]: https://docs.rs/rust-llm-tidy-fix/badge.svg
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg
[crates-io]: https://crates.io/crates/rust-llm-tidy-fix
[docs-rs]: https://docs.rs/rust-llm-tidy-fix
[ci-actions]: https://github.com/Sewer56/rust-llm-tidy/actions
