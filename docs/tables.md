# `tables` - align GFM tables

## What it does

Pads GitHub-Flavored Markdown table columns to a consistent width so the
pipe delimiters line up. Runs in `.md` files and in `.rs` doc-comment tables.

## Before

```markdown
| Name | Value |
| --- | --- |
| a | 1 |
| longname | 200 |
```

## After

```markdown
| Name     | Value |
| -------- | ----- |
| a        | 1     |
| longname | 200   |
```

## Config

```yaml
exclude:
  - paths: ["docs/compat/**/*.md"]
    rules: [tables]
```

```bash
rust-llm-tidy --include tables --dry-run README.md
rust-llm-tidy --exclude tables docs
```

## Change output

Every run reports each realignment it applies (or would apply under `--dry-run`)
as one record. In text mode the record prints to stderr:

```text
README.md:1: success[FIX]: realign table starting at line 1 (table)
```

In JSON mode the same record appears on stdout with `severity: "success"`:

```json
[
  {
    "path": "README.md",
    "line": 1,
    "severity": "success",
    "code": "FIX",
    "message": "realign table starting at line 1",
    "item_kind": "table",
    "item_name": null
  }
]
```

See [Change reporting](./lints.md#change-reporting) for the shared format.
