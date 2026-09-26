# `spacing` - blank lines between documented members

## What it does

Inserts a missing blank line between two members of a type body when
either member has docs or attributes. Each doc comment then reads with
its own member.

- On by default for Rust (`.rs`) and C# (`.cs`); no other language
  has it.
- Rust bodies: braced `struct`, `union`, `enum`, `trait`, and `impl`.
- C# bodies: `class`, `struct`, `interface`, `record`, and `enum`.
- Tuple structs and unit structs (Rust) and positional records (C#)
  are not checked.
- Undocumented neighbors stay packed.
- Members on the same line are exempt.

A member's docs, attributes, and attached comments belong to it: the
blank line goes before them. A comment trailing the previous member
stays on its line.

C# specifics: when a `#if` conditional leads the next member, the
blank line goes before the `#if`, or before a comment ahead of it.

`#region`/`#endregion` lines are skipped, and a gap across them is
still spaced: the blank line lands after the directive. Members
inside conditionals are unchecked.

Existing bytes are preserved; the op only inserts a line terminator -
carriage return/line feed (CRLF) where the file uses CRLF. A re-run
over the output changes nothing.

Files whose tree recovered from syntax errors are skipped without
failing the run. Declaration exclusions with `exclude_edits` are the
exception: they fail closed on damaged trees and fail the file.

The same exclusions otherwise protect a gap from being spaced.

## Before

```rust,ignore
/// An edge in the flow graph.
pub struct LocalEdge {
    /// Address of the instruction taking this path.
    pub source: u32,
    /// Address of the first instruction at the destination.
    pub target: u32,
}
```

## After

```rust,ignore
/// An edge in the flow graph.
pub struct LocalEdge {
    /// Address of the instruction taking this path.
    pub source: u32,

    /// Address of the first instruction at the destination.
    pub target: u32,
}
```

## Config

```yaml
# Whitelist: run only spacing on Rust and C# sources
include:
  - paths: ["src/**/*.rs", "src/**/*.cs"]
    rules: [spacing]

# Blacklist: never space generated code
exclude:
  - paths: ["**/generated/**"]
    rules: [spacing]
```

```bash
# CLI: run only spacing for this invocation
rust-llm-tidy --include spacing --dry-run src/lib.rs
# CLI: skip spacing for this invocation
rust-llm-tidy --exclude spacing src
```

Selections naming the retired `FMT001` code are rejected as unknown
rules.

`--dry-run` previews the inserts without writing and exits non-zero
when edits are proposed. `--checks-only` never spaces: it suppresses
transform ops.

The op runs after [reorder] and [vis], so it spaces the final member
order.

## Change output

Every run reports each insert it applies (or would apply under
`--dry-run`) as one record. In text mode the record prints to stderr:

```text
src/lib.rs:5: success[FIX]: insert blank line between `source` and `target` (struct)
```

The record's line is the line the blank line goes before. The message
names the two members; the parenthesized kind is the enclosing item.
In JSON mode the same record appears on stdout with
`severity: "success"`:

```json
[
  {
    "path": "src/lib.rs",
    "line": 5,
    "severity": "success",
    "code": "FIX",
    "message": "insert blank line between `source` and `target`",
    "item_kind": "struct",
    "item_name": null,
    "title": null
  }
]
```

See [Change reporting] for the shared format.
[Change reporting]: ./lints.md#change-reporting
[reorder]: ./reorder.md
[vis]: ./vis.md

## Library access

Use `rust_llm_tidy::rules::transform::spacing`.
For complete processing and project context, see [library entry points].

[library entry points]: architecture.md#library-entry-points
