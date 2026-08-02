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
| [`DOC006`]  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`/`...`).             |
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

### DOC006 - placeholder text

Doc comments on documentable items must not contain whole-word `TODO`,
`FIXME`, or `TBD` markers, or the literal `...`.

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

[`DOC001`]: #doc001---missing-documentation
[`DOC002`]: #doc002---missing-errors-section
[`DOC003`]: #doc003---vague-errors-section
[`DOC004`]: #doc004---missing-arguments-section
[`DOC005`]: #doc005---undocumented-parameter
[`DOC006`]: #doc006---placeholder-text
[`TEST001`]: #test001---non-behavioral-test-name
