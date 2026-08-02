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
