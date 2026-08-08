# `fences` - alternate nested fence delimiters

## What it does

When a fenced code block is nested inside another fenced block, the inner
fence uses the opposite delimiter (```` ``` ```` vs ```` ~~~ ````) so the
outer block does not close early. Runs in `.md` files and `.rs` doc comments.

## Before

```markdown
```
text

```rust
fn main() {}
```
```
```

## After

```markdown
```
text

~~~rust
fn main() {}
~~~
```
```

## Config

The repo's own `src/rust-llm-tidy-fix/README.MD` keeps a nested-fence
before/after example that `fences` would corrupt, so it is excluded:

```yaml
exclude:
  - paths:
      - "src/rust-llm-tidy-fix/README.MD"
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
    "item_name": null
  }
]
```

See [Change reporting](./lints.md#change-reporting) for the shared format.
