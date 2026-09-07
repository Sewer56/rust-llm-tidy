# `fences` - alternate nested fence delimiters

## What it does

When a fenced code block is nested inside another fenced block, the inner
fence uses the opposite delimiter.

- The delimiter pair is ```` ``` ```` vs ```` ~~~ ````.
- The outer block then does not close early.
- Runs in markdown-family files and in the line comments and doc comments
  of allowed code languages.

## Before

````markdown
~~~
text

```rust
fn main() {}
```
~~~
````

## After

````markdown
~~~
text

```rust
fn main() {}
```
~~~
````

## Config

This page keeps a nested-fence
before/after example that `fences` would corrupt, so it is excluded:

```yaml
exclude:
  - paths:
      - "docs/fences.md"
    rules:
      - fences
```

```bash
rust-llm-tidy --include fences --dry-run docs
rust-llm-tidy --exclude fences README.md
```

## Change output

Every run reports each fence flip it applies (or would apply under `--dry-run`)
as one record. In text mode the record prints to stderr:

```text
README.md:3: success[FIX]: flip nested fence at line 3 (fence)
```

In JSON mode the same record appears on stdout with `severity: "success"`:

```json
[
  {
    "path": "README.md",
    "line": 3,
    "severity": "success",
    "code": "FIX",
    "message": "flip nested fence at line 3",
    "item_kind": "fence",
    "item_name": null,
    "title": null
  }
]
```

See [Change reporting] for the shared format.

[Change reporting]: ./lints.md#change-reporting

## Library access

Use `rust_llm_tidy::rules::transform::fix_fences` for this rule.

For complete processing and project context, see [library entry
points](architecture.md#library-entry-points).
