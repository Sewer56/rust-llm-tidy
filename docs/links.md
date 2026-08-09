# `links` - hoist inline links to reference style

## What it does

Every eligible inline link `[text](url)` is replaced with the reference form
`[text]` plus a `[text]: url` definition, by default even when the link appears
only once. Runs in `.rs` doc comments and `.md` files.

- In `.rs` doc comments, each `[text]: url` definition is duplicated inside
  every doc comment that uses the label, so each comment is self-sufficient
  and `cargo doc` stays clean.
- In `.md` files, all definitions collect in one trailing block at the end of
  the document.

The hoist threshold defaults to 1 and is configurable via `links.min_occurrences`
(see the `.rust-llm-tidy.yml` header); raising it leaves a link inline
until it appears that many times. The configurable `links:` threshold ships in
a later release; this release hoists at the default threshold of 1.

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

The repo's own `src/cli/tests/**` embed markdown in Rust string literals. Such
files have no doc-comment lines, so `links` treats them as markdown context and
emits a `[text]: url` trailing line inside the `.rs` file, outside any literal.
That breaks compilation, so `links` is excluded for those paths:

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
(`line` is `null` for link records - no line applies). Both contexts share one
record format; hoisting `[A](http://x)` to `[A]` reports the same record whether
the link lives in a Rust doc comment or a markdown file:

```json
[
  {
    "path": "src/lib.rs",
    "line": null,
    "severity": "success",
    "code": "FIX",
    "message": "`[A](http://x)` -> `[A]`",
    "item_kind": "link",
    "item_name": null
  }
]
```

See [Change reporting](./lints.md#change-reporting) for the shared format.
