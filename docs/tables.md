# `tables` - align GFM tables

## What it does

Pads GitHub-Flavored Markdown table columns to a consistent width so the
pipe delimiters line up. Runs in markdown-family files and in the line
comments and doc comments of admitted code languages.

The tier's comment prefix is stripped and re-applied, so every row keeps
its marker and indent. See the [language matrix] for which extensions
run it.

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

Every run reports the realignments it applies (or would apply under
`--dry-run`) as one record per file. In text mode the record prints to stderr:

```text
README.md: success[FIX]: tables were aligned (table)
```

In JSON mode the same record appears on stdout with `severity: "success"`
(`line` is `null` for table records - one record covers the whole file):

```json
[
  {
    "path": "README.md",
    "line": null,
    "severity": "success",
    "code": "FIX",
    "message": "tables were aligned",
    "item_kind": "table",
    "item_name": null,
    "title": null
  }
]
```

See [Change reporting] for the shared format.
[Change reporting]: ./lints.md#change-reporting
[language matrix]: ./langs.md
