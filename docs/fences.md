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
