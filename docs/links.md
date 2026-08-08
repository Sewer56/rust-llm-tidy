# `links` - hoist repeated inline links

## What it does

When the same inline link `[text](url)` appears more than once in a file,
all occurrences are replaced with the reference form `[text]`, with a single
reference definition
`[text]: url` appended. Runs in `.md` files and `.rs` doc comments.

## Before

```markdown
see [A](http://x) and [A](http://x)
```

## After

```markdown
see [A] and [A]

[A]: http://x
```

## Config

The repo's own `src/cli/tests/**` embed markdown in Rust string literals, so
`links` would emit a `[text]: url` line outside the literal and break
compilation - it is excluded for those paths:

```yaml
exclude:
  - paths:
      - "src/cli/tests/**"
    rules:
      - links
```

```bash
rust-llm-tidy --include links --dry-run README.md
rust-llm-tidy --exclude links src
```

## Change output

Every run reports each hoisted link it applies (or would apply under
`--dry-run`) as one record showing the before -> after substitution. In text
mode the records print to stderr:

```text
README.md: success[FIX]: `[A](http://x)` -> `[A]` (link)
```

In JSON mode the same records appear on stdout with `severity: "success"`
(`line` is 0 for link records - no line applies):

```json
[
  {
    "path": "README.md",
    "line": 0,
    "severity": "success",
    "code": "FIX",
    "message": "`[A](http://x)` -> `[A]`",
    "item_kind": "link",
    "item_name": null
  }
]
```

See [Change reporting](./lints.md#change-reporting) for the shared format.
