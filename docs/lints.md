# `lints` - documentation and test-naming checks

## What it does

The `lints` op runs nine read-only checks (DOC001-DOC008 + TEST001). It is
on by default in the pipeline and never mutates files. Exits non-zero when
any error-severity finding is present (warnings do not fail).

The nine lint codes are sub-checks of `lints`; they stay individually
toggleable through the same rule namespace as the ops.

So `exclude: [{rules: [DOC001]}]` turns off just missing-docs and
`exclude: [{rules: [lints]}]` turns off all linting.

Which codes run depends on the language: Rust runs all nine, the
markdown family runs the two text checks (DOC007, DOC008), and C# runs
DOC001-DOC006 and TEST001 against XML doc comments. See [lints for C#]
for the C# dialect.

## Codes

| Code        | Severity | Fires when                                                                        |
| ----------- | -------- | --------------------------------------------------------------------------------- |
| [`DOC001`]  | Error    | A non-private item has no doc comment (`///`, `/** ... */`, or `#[doc = "..."]`). |
| [`DOC002`]  | Error    | A `pub fn` returning `Result` has no `# Errors` section.                          |
| [`DOC003`]  | Warning  | A `# Errors` section names no concrete error variant.                             |
| [`DOC004`]  | Warning  | A `pub fn` with parameters has no `# Arguments` section.                          |
| [`DOC005`]  | Warning  | A `# Arguments` section does not mention every parameter name.                    |
| [`DOC006`]  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`).                   |
| [`DOC007`]  | Error    | A doc paragraph over 240 chars of full text (bullets warn).                       |
| [`DOC008`]  | Warning  | A doc line over 80 chars of full text (code blocks, tables, link defs exempt).    |
| [`TEST001`] | Warning  | A test fn uses `test`, `test_*`, `case_*`, or `test1`-style names.                |

## Examples

Each example shows the smallest common fix for its lint.

### DOC001 - missing documentation

Non-private documentable items need a `///`, `/** ... */`, or
`#[doc = "..."]` comment. Private items, modules, imports, impls, macros,
macro invocations, uncategorized items, and `extern crate` items are not
checked.

Before:

```rust
pub fn load() {}
```

After:

```rust
/// Loads the configured data.
pub fn load() {}
```

#### DOC001 CLI output

Running `lints` against the Before code:

```text
$ rust-llm-tidy --no-config --include DOC001 src/lib.rs
src/lib.rs:1: error[DOC001]: non-private item is missing a doc comment (fn `load`)
Error: found 1 error(s)
```

`DOC001` is error-severity, so the run exits non-zero.

### DOC002 - missing `# Errors` section

Public functions returning `Result` need an `# Errors` section.

Before:

```rust
/// Loads the configured data.
pub fn load() -> Result<(), std::io::Error> {
    Ok(())
}
```

After:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// Returns an I/O error when data cannot be loaded.
pub fn load() -> Result<(), std::io::Error> {
    Ok(())
}
```

#### DOC002 CLI output

```text
$ rust-llm-tidy --no-config --include DOC002 src/lib.rs
src/lib.rs:3: error[DOC002]: pub fn returning Result is missing a `# Errors` doc section (fn `load`)
Error: found 1 error(s)
```

`DOC002` is error-severity, so the run exits non-zero.

### DOC003 - vague `# Errors` section

When non-empty, an `# Errors` section must contain text with `[` or `::`, the
heuristic used to recognize a concrete error variant. Sections with an empty
body slice are ignored; whitespace-only bodies still warn.

Before:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// Returns an error if loading fails.
pub fn load() -> Result<(), std::io::Error> {
    Ok(())
}
```

After:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// Returns [`Error::Unavailable`] when data cannot be loaded.
pub fn load() -> Result<(), Error> {
    Ok(())
}

enum Error {
    Unavailable,
}
```

#### DOC003 CLI output

```text
$ rust-llm-tidy --no-config --include DOC003 src/lib.rs
src/lib.rs:7: warning[DOC003]: `# Errors` section does not name any concrete error variant (fn `load`)
```

`DOC003` is warning-severity, so the run exits 0.

### DOC004 - missing `# Arguments` section

Public functions with named parameters need a recognized argument section:
`# Arguments`, `# Argument`, `# Parameters`, `# Parameter`, `# Params`, or
`# Param` (case-insensitive).

Before:

```rust
/// Greets a user.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}")
}
```

After:

```rust
/// Greets a user.
///
/// # Arguments
///
/// * `name` - Name to greet.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}")
}
```

#### DOC004 CLI output

```text
$ rust-llm-tidy --no-config --include DOC004 src/lib.rs
src/lib.rs:1: warning[DOC004]: pub fn with parameters is missing a `# Arguments` doc section (fn `greet`)
```

`DOC004` is warning-severity, so the run exits 0.

### DOC005 - undocumented parameter

An `# Arguments`, `# Argument`, `# Parameters`, `# Parameter`, `# Params`, or
`# Param` section must mention every named non-`self` parameter recognized by
the parser.

Before:

```rust
/// Formats text.
///
/// # Arguments
///
/// * `text` - Text to format.
pub fn format(text: &str, width: usize) -> String {
    format!("{text:width$}")
}
```

After:

```rust
/// Formats text.
///
/// # Arguments
///
/// * `text` - Text to format.
/// * `width` - Output width.
pub fn format(text: &str, width: usize) -> String {
    format!("{text:width$}")
}
```

#### DOC005 CLI output

```text
$ rust-llm-tidy --no-config --include DOC005 src/lib.rs
src/lib.rs:1: warning[DOC005]: parameter(s) not documented in the `# Arguments` section: `width` (fn `format`)
```

`DOC005` is warning-severity, so the run exits 0.

### DOC006 - placeholder text

Doc comments on documentable items must not contain whole-word `TODO`,
`FIXME`, or `TBD` markers.

Before:

```rust
/// TODO: document loading behavior...
pub fn load() {}
```

After:

```rust
/// Loads data from configured storage.
pub fn load() {}
```

#### DOC006 CLI output

```text
$ rust-llm-tidy --no-config --include DOC006 src/lib.rs
src/lib.rs:1: warning[DOC006]: doc comment contains placeholder text (TODO/FIXME/TBD) (fn `load`)
```

`DOC006` is warning-severity, so the run exits 0.

### DOC007 - oversized paragraph

A paragraph of doc text over 240 chars is an error. A bullet over
240 chars warns instead and recommends one checkable action of at most 160
chars. Nested bullets are separate paragraphs.

Both checks strip leading whitespace, the comment marker (`//`, `///`, `//!`
in Rust), and one following space. They run on `.rs` files and the markdown
family.

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

#### DOC007 CLI output

```text
$ rust-llm-tidy --no-config --include DOC007 src/lib.rs
src/lib.rs:1: error[DOC007]: paragraph is 243 chars long.
  - Paragraphs over 240 chars outlast a short attention span.
  - Split it at the nearest idea change with a blank line.
  - Convert list-like paragraphs into bullets.
  - Keep each bullet to one checkable action of at most 160 chars.
  - Move remarks into their own sections.
  - The check skips code blocks, tables, headings, signature lines, and link definitions. (file)
Error: found 1 error(s)
```

`DOC007` is error-severity for prose, so the run exits non-zero; bullets
exit 0.

### DOC008 - long line

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

#### DOC008 CLI output

```text
$ rust-llm-tidy --no-config --include DOC008 README.md
README.md:1: warning[DOC008]: line is 104 chars long.
  - Lines over 80 chars strain short attention spans and need wide monitors.
  - Split it at the nearest idea change with a blank line.
  - Code spans, URLs, and link targets count.
  - Code blocks, table rows, and link definitions are exempt. (file)
```

`DOC008` is warning-severity, so the run exits 0.

### TEST001 - non-behavioral test name

Test-attributed functions should describe behavior, not use `test`, `test_*`,
`case_*`, or `test` followed by digits.

Before:

```rust
#[test]
fn test_foo() {
    assert_eq!(parse("ok"), Ok(()));
}
```

After:

```rust
#[test]
fn parse_returns_ok_for_valid_input() {
    assert_eq!(parse("ok"), Ok(()));
}
```

#### TEST001 CLI output

```text
$ rust-llm-tidy --no-config --include TEST001 src/lib.rs
src/lib.rs:1: warning[TEST001]: test function `test_foo` should use a behavioral name (subject_should_expectation_when_condition), not a `test_*` or `case_*` prefix (fn `test_foo`)
```

`TEST001` is warning-severity, so the run exits 0.

## Config

```yaml
# Turn off all linting
exclude:
  - rules: [lints]            # paths omitted -> implied ["**"]

# Turn off just missing-docs
exclude:
  - rules: [DOC001]
```

```bash
# Run only linting for this invocation
rust-llm-tidy --include lints src
# Skip linting for this invocation
rust-llm-tidy --exclude lints src
```

## JSON output

```bash
# Print findings and change records as a single JSON array on stdout
rust-llm-tidy --output-mode json src
# `--json` is an alias for `--output-mode json`
rust-llm-tidy --json src
```

Print every lint finding and change record as one JSON array on stdout, in both
in-place and `--dry-run` runs. Prints `[]` when there are no findings or
changes, and still prints the document when the run exits non-zero:

```json
[
  {
    "path": "src/lib.rs",
    "line": 1,
    "severity": "error",
    "code": "DOC001",
    "message": "non-private item is missing a doc comment",
    "item_kind": "fn",
    "item_name": "load",
    "title": "missing documentation"
  }
]
```

Fields:

- `severity` - `"error"` or `"warning"` for lint findings, `"success"` for
  change records (applied or would-be changes)
- `line` - 1-based item start line; `null` when the record has no specific
  line (e.g. link/table fixes)
- `item_name` - item name, `null` when unnamed
- `title` - friendly per-code title for lint findings, `null` for change
  records
- `path`, `code`, `message`, `item_kind` - as in plaintext

In JSON mode the plaintext `path:line: sev[CODE]: ...` diagnostics are not
printed to stderr. Change records and lint findings are folded into the same
document, in both in-place and `--dry-run` runs.

## Change reporting

Every run reports the edits each enabled op makes (in-place) or would make
(`--dry-run`). `--dry-run` previews the changes without writing them. In text
mode every edit is one plaintext line on stderr:

```text
src/lib.rs:20: success[REORDER]: rearrange fn a_main from pos 2 to pos 1 (before b_helper) (fn `a_main`)
```

The line shape is:

```text
path:line: success[CODE]: message (item_kind `item_name`)
```

Fix records are unnamed, so they print like `1: success[FIX]: realign table
starting at line 1 (table)` without a trailing name. In JSON mode the same
records appear on stdout as one JSON array with `severity: "success"`:

```json
[
  {
    "path": "src/lib.rs",
    "line": 20,
    "severity": "success",
    "code": "REORDER",
    "message": "rearrange fn a_main from pos 2 to pos 1 (before b_helper)",
    "item_kind": "fn",
    "item_name": "a_main",
    "title": null
  }
]
```

Each operation's concrete output in both modes is shown in its own doc page.

[`DOC001`]: #doc001---missing-documentation
[`DOC002`]: #doc002---missing-errors-section
[`DOC003`]: #doc003---vague-errors-section
[`DOC004`]: #doc004---missing-arguments-section
[`DOC005`]: #doc005---undocumented-parameter
[`DOC006`]: #doc006---placeholder-text
[`DOC007`]: #doc007---oversized-paragraph
[`DOC008`]: #doc008---long-line
[`TEST001`]: #test001---non-behavioral-test-name
[lints for C#]: ./lints/csharp.md
