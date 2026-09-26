# `spacing` - blank lines between documented members

## What it does

Adds a blank line between adjacent members when either has docs or
attributes. Runs by default on Rust (`.rs`) and C# (`.cs`) files.

## Before

```rust,ignore
struct Edge {
    /// Address of the source instruction.
    source: u32,
    /// Address of the destination instruction.
    target: u32,
}
```

## After

```rust,ignore
struct Edge {
    /// Address of the source instruction.
    source: u32,

    /// Address of the destination instruction.
    target: u32,
}
```

The line goes before the next member's docs or attributes, not between
the comment and the member it describes.

The fix covers Rust braced structs, unions, enums, traits and impls,
and C# classes, structs, interfaces, records and enums. It skips:

- Existing blank lines, undocumented pairs and same-line members.
- Rust tuple and unit structs, and C# positional records.
- Members inside C# `#if` blocks. A leading `#if` stays with the next
  member.
- Files with syntax-error trees. With `exclude_edits` declaration
  protection, a damaged tree fails the file instead.

It inserts only a line ending, matching the preceding line (including
CRLF). Running it again changes nothing.

## Config

Use `include` to run only spacing for matching files:

```yaml
include:
  - paths: ["src/**/*.rs", "src/**/*.cs"]
    rules: [spacing]
```

Or use `exclude` to skip spacing in generated code:

```yaml
exclude:
  - paths: ["**/generated/**"]
    rules: [spacing]
```

```bash
# Preview spacing alone
rust-llm-tidy --include spacing --dry-run src/lib.rs
# Skip spacing
rust-llm-tidy --exclude spacing src
```

Do not combine `include` and `exclude` in one config. `--dry-run` exits
non-zero if it finds edits; `--checks-only` runs no transforms.

Spacing runs after [reorder] and [vis].
The retired `FMT001` warning is not selectable; `spacing` is a fix.

## Change output

Each inserted line reports a change, including under `--dry-run`:

```text
src/lib.rs:5: success[FIX]: insert blank line between `source` and `target` (struct)
```

The line number points to the next member's first attached line in the
input. JSON mode reports the same change on stdout:

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

See [change reporting] for the shared format.

## Library access

Use `rust_llm_tidy::rules::transform::spacing::fix_spacing` for a source
buffer. It returns the text and one edit per inserted line.

For the full pipeline and project context, see [library entry points].

[reorder]: ./reorder.md
[vis]: ./vis.md
[change reporting]: ./lints.md#change-reporting
[library entry points]: architecture.md#library-entry-points
