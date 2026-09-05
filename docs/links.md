# `links` - hoist inline links to reference style

## What it does

Replaces every eligible inline link `[text](url)` with the reference form
`[text]` plus a `[text]: url` definition. Runs in `///` and `//!` doc
comments and markdown-family files only.

- Doc comments: each definition is duplicated into every comment using
  the label, so `cargo doc` stays clean.
- Markdown: definitions collect in one trailing block.

Eligible link text is non-blank and free of `[`/`]` bytes, and the open `[`
must be unescaped (`\[x](u)` is literal text). Other links stay inline, e.g. a
badge's outer `[![alt](img)](url)` link; only its flat inner image hoists.

The hoist threshold defaults to 1 and is configurable via
`links.min_occurrences` (see the `.rust-llm-tidy.yml` header); raising it
leaves a link inline until it appears that many times.

`links` is idempotent: running it on its own output changes nothing.

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
src/lib.rs: success[FIX]: `[A](http://x)` -> `[A]` (link)
```

In JSON mode the same records appear on stdout with `severity: "success"`
(`line` is `null` for link records - no line applies):

```json
[
  {
    "path": "src/lib.rs",
    "line": null,
    "severity": "success",
    "code": "FIX",
    "message": "`[A](http://x)` -> `[A]`",
    "item_kind": "link",
    "item_name": null,
    "title": null
  }
]
```

See [Change reporting] for the shared format.

[Change reporting]: ./lints.md#change-reporting
