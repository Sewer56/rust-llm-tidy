# Text lints - TEXT001 and TEXT002

The text lints are `lints` sub-checks ([`lints`]) run on raw text:
comment-stripped for programming languages, raw for markdown and text
files.

Leading whitespace (a docstring's common indentation), the comment
marker run, and one following space strip before measuring; each source
keeps its own paragraphs.

## Sources per tier

| Tier                    | Measured doc text                                                                                       |
| ----------------------- | ------------------------------------------------------------------------------------------------------- |
| Markdown family         | the whole file's prose                                                                                  |
| Rust                    | `//`, `///`, `//!` line comments, outer `/** */` block docs, `#[doc = "..."]` values (like `///` lines) |
| C#                      | `///` XML doc comments: text-node inner text                                                            |
| Python                  | module, class, and function docstrings; `#` comments                                                    |
| Comment-marker families | line comments and the family's block forms (below)                                                      |

Block forms measured per family:

- `//` family and sql: `/** */` and `/* */`.
- lua: `--[[ ]]`.
- hs and elm: `{- -}`.
- el, lisp, scm: `#| |#`.
- m: `%{ %}` alone on its line.

Never measured:

- Rust: plain `/* */` comments and the inner `/*! */` and
  `#![doc = "..."]` forms.
- Python: triple-quoted strings that are not docstrings.
- The marker families: string content, heredoc payload, and code lines.
- Block docs: `*` continuations and `@tag` name tokens
  (`@param name`).
- Python `>>>` doctest examples: source lines, `...` continuations,
  and expected output, until the blank line ending the example.

Python's producer and the marker families' comment lexicon fail closed:
a file they cannot attribute safely produces no findings rather than
guesses.

## TEXT001 - oversized paragraph

A paragraph of doc text over 240 chars is an error. A bullet over
240 chars warns instead and recommends one checkable action of at most 160
chars. Nested bullets are separate paragraphs.

Code blocks, tables, headings, signature lines, and link definitions are
exempt as whole lines and end a paragraph.

Before:

```rust
/// Loads the configured data from disk, parses it, validates every field
/// against the schema, resolves relative paths against the config
/// directory, retries transient failures with bounded backoff, and logs a
/// one-line summary when the load settles.
pub fn load() {}
```

After:

```rust
/// Loads the configured data from disk and parses it.
///
/// - Validates every field against the schema.
/// - Resolves relative paths against the config directory.
/// - Retries transient failures with bounded backoff.
/// - Logs a one-line summary when the load settles.
pub fn load() {}
```

### TEXT001 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT001 src/lib.rs
src/lib.rs:1: error[TEXT001]: paragraph is 243 chars long.
  - Paragraphs over 240 chars outlast a short attention span.
  - Split it at the nearest idea change with a blank line.
  - Convert list-like paragraphs into bullets.
  - Keep each bullet to one checkable action of at most 160 chars.
  - Move remarks into their own sections.
  - The check skips code blocks, tables, headings, signature lines, and link definitions. (file)
Error: found 1 error(s)
```

`TEXT001` is error-severity for prose, so the run exits non-zero; bullets
exit 0.

## TEXT002 - long line

A doc line over 80 chars is a warning. Lines count in full: code spans,
URLs, and link targets included.

Code blocks, table rows, and link reference definitions are exempt.

Before:

```md
The config is discovered by walking up from the current directory to the repo root, checking each level.
```

After:

```md
The config is discovered by walking up from the current directory to the repo
root, checking each level.
```

### TEXT002 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT002 README.md
README.md:1: warning[TEXT002]: line is 104 chars long.
  - Lines over 80 chars strain short attention spans and need wide monitors.
  - Split it at the nearest idea change with a blank line.
  - Code spans, URLs, and link targets count.
  - Code blocks, table rows, and link definitions are exempt. (file)
```

`TEXT002` is warning-severity, so the run exits 0.

[`lints`]: ./lints.md
