# `links` - hoist inline links to reference style

## What it does

Replaces every eligible inline link `[text](url)` with the reference form
`[text]` plus a `[text]: url` definition.

- Rust: definitions and occurrence thresholds stay within each comment group.
- Markdown/plaintext: definitions collect at the end; fenced code is skipped.

See [text transformation safety] for supported files and comment boundaries.

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

Exclude files whose examples must keep inline link syntax:

```yaml
exclude:
  - paths:
      # Hey that's this file!
      - "docs/links.md"
    rules:
      - links
```

```bash
rust-llm-tidy --include links --dry-run README.md
rust-llm-tidy --exclude links src
```

## Change output

Every run reports each hoisted link it applies as one record showing the
before -> after substitution.

- Under `--dry-run`, the same records report links it would apply.
- In text mode the records print to stderr:

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

## Library access

`rust_llm_tidy::rules::transform::fix_links` is a low-level text engine,
not a safe arbitrary-source API.

See [text transformation safety] and [library entry points].

[library entry points]: architecture.md#library-entry-points
[text transformation safety]: ../README.MD#text-transformation-safety
