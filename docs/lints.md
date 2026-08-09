# `lints` - documentation and test-naming checks

## What it does

The `lints` op runs seven read-only checks (DOC001-DOC006 + TEST001). It is
on by default in the pipeline and never mutates files. Exits non-zero when
any error-severity finding is present (warnings do not fail).

The seven lint codes are sub-checks of `lints`; they stay individually
toggleable through the same rule namespace as the ops, so
`exclude: [{rules: [DOC001]}]` turns off just missing-docs and
`exclude: [{rules: [lints]}]` turns off all linting.

## Codes

| Code        | Severity | Fires when                                                                        |
| ----------- | -------- | --------------------------------------------------------------------------------- |
| [`DOC001`]  | Error    | A non-private item has no doc comment (`///`, `/** ... */`, or `#[doc = "..."]`). |
| [`DOC002`]  | Error    | A `pub fn` returning `Result` has no `# Errors` section.                          |
| [`DOC003`]  | Warning  | A `# Errors` section names no concrete error variant.                             |
| [`DOC004`]  | Warning  | A `pub fn` with parameters has no `# Arguments` section.                          |
| [`DOC005`]  | Warning  | A `# Arguments` section does not mention every parameter name.                    |
| [`DOC006`]  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`).                   |
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
    "item_name": "load"
  }
]
```

Fields:

- `severity` - `"error"` or `"warning"` for lint findings, `"success"` for change records (applied or would-be changes)
- `line` - 1-based item start line; `null` when the record has no specific line (e.g. link/table fixes)
- `item_name` - item name, `null` when unnamed
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

The line shape is `path:line: success[CODE]: message (item_kind `item_name`)`.
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
    "item_name": "a_main"
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
[`TEST001`]: #test001---non-behavioral-test-name
