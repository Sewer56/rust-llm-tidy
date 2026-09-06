# `lints` - documentation, text, and test-naming checks

## What it does

The `lints` op runs read-only checks.

- It is on by default in the pipeline and never mutates files.
- Exits non-zero when any error-severity finding is present (warnings and
  hints do not fail).

The lint codes are sub-checks of `lints`; they stay individually
toggleable through the same rule namespace as the ops.

So `exclude: [{rules: [DOC001]}]` turns off just missing-docs and
`exclude: [{rules: [lints]}]` turns off all linting.

Which codes run depends on the language:

- Rust: every code, read from a tree-sitter parse.
- C#: every code, read from a tree-sitter parse, evaluated against XML
  doc comments ([lints for C#]).

Languages other than Rust and C# run the text lints only ([text lints]):

- Markdown family: read from the raw file.
- Python: read from a tree-sitter-python parse.
- Comment-marker families (`//`, `#`, `--`, `;`, `%`): read from the
  comment lexicon.

## Codes

| Code        | Severity | Fires when                                                                        |
| ----------- | -------- | --------------------------------------------------------------------------------- |
| [`DOC001`]  | Error    | A non-private item has no doc comment (`///`, `/** ... */`, or `#[doc = "..."]`). |
| [`DOC002`]  | Error    | A `pub fn` returning `Result` has no `# Errors` section.                          |
| [`DOC003`]  | Warning  | A `# Errors` section names no concrete error variant.                             |
| [`DOC004`]  | Warning  | A `pub fn` with parameters has no `# Arguments` section.                          |
| [`DOC005`]  | Warning  | A `# Arguments` section does not mention every parameter name.                    |
| [`DOC006`]  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`).                   |
| [`DOC008`]  | Error    | An `# Errors` section lists enum variants out of alphabetical order.              |
| [`TEXT001`] | Error    | A doc paragraph over 240 chars of full text (bullets warn).                       |
| [`TEXT002`] | Warning  | A doc line over 80 chars of full text (code blocks, tables, link defs exempt).    |
| [`TEXT003`] | Warning  | A doc sentence over 25 words (words join across wrapped lines).                   |
| [`TEXT004`] | Warning  | A doc opener with 3+ sentences or over 160 chars (file, item, or heading).        |
| [`TEXT005`] | Warning  | A fenced code block opens with no tag or bare `ignore` (all markdown prose).      |
| [`TEXT006`] | Hint     | A doc line has a shorter alternative for a word, phrase, or filler.               |
| [`TEXT007`] | Warning  | A doc line holds a passive construction or a past-behavior narration marker.      |
| [`TEST001`] | Warning  | A test fn uses `test`, `test_*`, `case_*`, or `test1`-style names.                |

## Examples

Each example shows the smallest common fix for its lint.

### DOC001 - missing documentation

Non-private documentable items need a doc comment.

- Accepted forms: `///`, `/** ... */`, or `#[doc = "..."]`.
- Private items, modules, imports, impls, macros, macro invocations,
  uncategorized items, and `extern crate` items are not checked.

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

When non-empty, an `# Errors` section must contain text with `[` or `::`.

- `[` or `::` is the heuristic used to recognize a concrete error variant.
- Sections with an empty body slice are ignored.
- Whitespace-only bodies still warn.

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

Public functions with named parameters need a recognized argument section
(case-insensitive):

- `# Arguments`
- `# Argument`
- `# Parameters`
- `# Parameter`
- `# Params`
- `# Param`

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

The argument section must mention every named non-`self` parameter
recognized by the parser.

Recognized sections (case-insensitive):

- `# Arguments`
- `# Argument`
- `# Parameters`
- `# Parameter`
- `# Params`
- `# Param`

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

### DOC008 - error variants out of alphabetical order

In `# Errors`, sort links to returned error variants by case-sensitive Rust
`str` order.

Only checks `` [`Enum::Variant`] `` links (path prefixes allowed)
when the error type resolves to a top-level enum in the same file.

Before:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// - [`Error::NotFound`] when data cannot be loaded
/// - [`Error::Denied`] when access is refused
/// - [`Error::InvalidFormat`] when data has an unsupported format
pub fn load() -> Result<(), Error> {
    Ok(())
}

enum Error {
    Denied,
    InvalidFormat,
    NotFound,
}
```

After:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// - [`Error::Denied`] when access is refused
/// - [`Error::InvalidFormat`] when data has an unsupported format
/// - [`Error::NotFound`] when data cannot be loaded
pub fn load() -> Result<(), Error> {
    Ok(())
}

enum Error {
    Denied,
    InvalidFormat,
    NotFound,
}
```

#### DOC008 CLI output

```text
$ rust-llm-tidy --no-config --include DOC008 src/lib.rs
src/lib.rs:1: error[DOC008]: `# Errors` lists variants of `Error` out of alphabetical order (fn `load`)
Error: found 1 error(s)
```

`DOC008` is error-severity, so the run exits non-zero.

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

Every lint finding and change record is printed as one JSON array on stdout,
in both in-place and `--dry-run` runs.

- Prints `[]` when there are no findings or changes.
- Still prints the document when the run exits non-zero.

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

- `severity` - `"error"`, `"warning"`, or `"hint"` for lint findings,
  `"success"` for change records (applied or would-be changes)
- `line` - 1-based item start line; `null` when the record has no
  specific line (e.g. link/table fixes)
- `item_name` - item name, `null` when unnamed
- `title` - friendly per-code title for lint findings, `null` for change
  records
- `path`, `code`, `message`, `item_kind` - as in plaintext

In JSON mode the plaintext `path:line: sev[CODE]: ...` diagnostics are not
printed to stderr. Change records and lint findings are folded into the same
document, in both in-place and `--dry-run` runs.

## Hints

`hint` is an advisory severity for suggestions an LLM or human may want to
investigate, such as a possible pre-allocation.

- Hints never gate the exit code; only `error` findings do.
- Text mode prints them in a separate group at the end, in the usual
  `path:line: hint[CODE]: ...` shape.
- JSON mode records them with `severity: "hint"` and the usual lint fields.

## Change reporting

Every run reports the edits each enabled op makes (in-place) or would make
(`--dry-run`).

- `--dry-run` previews the changes without writing them.
- In text mode every edit is one plaintext line on stderr:

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
[`DOC008`]: #doc008---error-variants-out-of-alphabetical-order
[`TEXT001`]: ./text-lints.md#text001---oversized-paragraph
[`TEXT002`]: ./text-lints.md#text002---long-line
[`TEXT003`]: ./text-lints.md#text003---long-sentence
[`TEXT004`]: ./text-lints.md#text004---header-opener-shape
[`TEXT005`]: ./text-lints.md#text005---fenced-code-block-without-a-language-tag
[`TEXT006`]: ./text-lints.md#text006---verbose-synonyms
[`TEXT007`]: ./text-lints.md#text007---passive-construction-and-past-behavior-narration
[`TEST001`]: #test001---non-behavioral-test-name
[lints for C#]: ./languages/lints/csharp.md
[text lints]: ./text-lints.md

## Library access

Use `rust_llm_tidy::rules::lint`.
For complete processing and project context, see [library entry points].

[library entry points]: architecture.md#library-entry-points
