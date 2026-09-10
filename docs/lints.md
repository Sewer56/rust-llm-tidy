# `lints` - read-only source and documentation checks

## What it does

The `lints` op runs read-only checks.

- It is on by default in the pipeline and never mutates files.
- Exits non-zero when any error-severity finding is present (warnings and
  hints and reminders do not fail).

The lint codes are sub-checks of `lints`; they stay individually
toggleable through the same rule namespace as the ops.

So `exclude: [{rules: [DOC001]}]` turns off just missing-docs and
`exclude: [{rules: [lints]}]` turns off all linting.

Which codes run depends on the language:

- Rust: documentation, text, test, module, length, and symbol checks.
- C#: checks XML documentation, test names, and test summary comments
  ([lints for C#]).
- Python: checks module docstrings ([`DOC009`]).

Text lints for other languages use these sources ([text lints]):

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
| [`DOC009`]  | Error    | A module file has no top-level module docs (`//!` in Rust, docstring in Python).  |
| [`TEXT001`] | Error    | A doc paragraph over 240 chars of full text (bullets warn).                       |
| [`TEXT002`] | Warning  | A doc line over 80 chars (trailing URL, code blocks, tables, link defs exempt).   |
| [`TEXT003`] | Warning  | A doc sentence over 25 words (words join across wrapped lines).                   |
| [`TEXT004`] | Warning  | A doc opener with 3+ sentences or over 160 chars (file, item, or heading).        |
| [`TEXT005`] | Warning  | A fenced code block opens with no tag or bare `ignore` (all markdown prose).      |
| [`TEXT006`] | Hint     | A doc line has a shorter alternative for a word, phrase, or filler.               |
| [`TEXT007`] | Reminder | A doc line may hold passive voice or implementation history.                      |
| [`TEXT008`] | Warning  | A bullet list exceeds 10 source lines.                                            |
| [`TEST001`] | Warning  | A test fn uses `test`, `test_*`, `case_*`, or `test1`-style names.                |
| [`TEST002`] | Reminder | A test fn carries no comment above its attributes.                                |
| [`MOD001`]  | Warning  | A code file exceeds `module_size.max_lines` (default 500).                        |
| [`MOD002`]  | Error    | A `use` inside a function body lacks its own `#[cfg]` attribute.                  |
| [`MOD003`]  | Hint     | A path includes the full namespace.                                               |
| [`LEN001`]  | Hint     | A Rust fn body exceeds `method_length.max_lines` (default 100).                   |
| [`SYM`]     | Reminder | A configured text or symbol hint matches; severity is configurable.               |
| [`DUP001`]  | Reminder | Five meaningful lines repeat at three same-file sites, including a changed copy.  |

## Reporting scope

- Reminders default to changed lines; other severities default to whole files.
- Set `lint_scopes: {DOC001: changed_lines}` to limit a lint to changed lines.
- Use `--all-lines` to report every severity across whole files.
- Priority: `--all-lines` > symbol `scope` > `lint_scopes` > severity default.
  Scopes affect findings, not file selection or edits. Only errors fail the run.

### Git baseline

- Default: compare working files with local `HEAD`.
- `--diff-base main` compares current files with their common ancestor
  with `main`.
  You can also set the reference via `RUST_LLM_TIDY_DIFF_BASE`.
- No paths? Check changed and untracked files, or scan the current directory
  with `--diff-base`.
- Without Git history, skip changed-line findings and warn.
  An invalid `--diff-base` stops the run.

## Examples

Examples show fixes without changing the intended behavior.

DOC checks ask for verified contracts, not invented errors or parameters.
Text fixes should preserve meaning, conditions, guarantees, and code
identifiers.

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
src/lib.rs:1: error[DOC001]: non-private item is missing a doc comment.

Why: Readers need its purpose and contract without tracing the implementation.

Suggestions:
- Add `///` docs describing its purpose and actual contract, based on the implementation and relevant callers. (fn `load`)
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
/// This function cannot return an error.
pub fn load() -> Result<(), std::io::Error> {
    Ok(())
}
```

#### DOC002 CLI output

```text
$ rust-llm-tidy --no-config --include DOC002 src/lib.rs
src/lib.rs:1: error[DOC002]: pub fn returning Result is missing a `# Errors` doc section.

Why: Readers need to understand possible failures and when they occur.

Suggestions:
- Add a `# Errors` section describing each error the implementation can return and its specific trigger.
- If it cannot return an error, say so.
- Do not invent errors or change behavior to satisfy this lint. (fn `load`)
Error: found 1 error(s)
```

`DOC002` is error-severity, so the run exits non-zero.

### DOC003 - vague `# Errors` section

For a public function returning `Result`, an existing `# Errors` section warns
unless its body contains `::`. Empty and whitespace-only bodies also warn.

Before:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// Returns an error.
pub fn load() -> Result<(), Error> {
    Err(Error::Unavailable)
}

enum Error {
    Unavailable,
}
```

After:

```rust
/// Loads the configured data.
///
/// # Errors
///
/// Returns [`Error::Unavailable`] on every call because loading is unavailable.
pub fn load() -> Result<(), Error> {
    Err(Error::Unavailable)
}

enum Error {
    Unavailable,
}
```

#### DOC003 CLI output

```text
$ rust-llm-tidy --no-config --include DOC003 src/lib.rs
src/lib.rs:1: warning[DOC003]: `# Errors` section has no `::` path naming a concrete error variant.

Why: Concrete error names help readers connect failure conditions to handling code.

Suggestions:
- Document each error the implementation can return and its specific trigger.
- Use variant paths where they exist.
- If the error type has no variants or the function cannot fail, document that contract.
- Do not invent variants or change behavior to silence this warning. (fn `load`)
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
src/lib.rs:1: warning[DOC004]: pub fn with parameters is missing a `# Arguments` doc section.

Why: Readers need parameter roles and constraints to supply appropriate inputs.

Suggestions:
- Add a `# Arguments` section describing the existing parameters and their actual roles and constraints.
- Do not change the signature or behavior to satisfy this lint. (fn `greet`)
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
src/lib.rs:1: warning[DOC005]: parameter(s) not documented in the `# Arguments` section: `width`.

Why: Omitted parameters leave readers guessing how to supply those inputs.

Suggestions:
- Describe these existing parameters and their actual roles and constraints.
- Do not add parameters or change behavior to satisfy this lint. (fn `format`)
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
src/lib.rs:1: warning[DOC006]: doc comment contains placeholder text (TODO/FIXME/TBD).

Why: Placeholders leave readers without an explanation of current behavior.

Suggestions:
- Replace it with documentation of the implemented behavior, not a promise of future behavior. (fn `load`)
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
src/lib.rs:1: error[DOC008]: `# Errors` lists variants of `Error` out of alphabetical order.

Why: Alphabetical entries help readers locate a known error variant.

Suggestions:
- Reorder the documented entries by variant name, keeping each trigger with its variant.
- Do not reorder the enum or change error behavior. (fn `load`)
Error: found 1 error(s)
```

`DOC008` is error-severity, so the run exits non-zero.

### DOC009 - module file without top-level docs

A module file with top-level content needs module docs, reported once at
line 1.

- Rust: a `//!` line before the first top-level item; `///` on the
  first item does not count.
- Python: a module docstring as the first statement.
- Empty modules never fire; there is no module purpose to document.

Before:

```rust
pub fn load() {}
```

After:

```rust
//! Loads the configured data.
pub fn load() {}
```

#### DOC009 CLI output

```text
$ rust-llm-tidy --no-config --include DOC009 src/lib.rs
src/lib.rs:1: error[DOC009]: module file is missing `//!` module docs.

Why: A purpose-first header helps readers understand the module
without reading its implementation.

Suggestions:
- Read the module and relevant callers; document only supported facts.
- Start with one concise sentence explaining what the module does and why.
  Do not just restate its name. A simple module needs no more.
- If more detail is useful, put it below the summary, separated by a blank doc line.
- Use that detail to outline major responsibilities, entry points, or non-obvious constraints.
- Use bullets for multiple topics.
- Link to item docs instead of repeating their details.
- Add `//!` docs before the first top-level item.
- For a module root (`mod.rs`, or `foo.rs` with child modules), identify
  main entry points and relevant child-module responsibilities.
- Keep this change to header writing; do not move code. (file)
Error: found 1 error(s)
```

`DOC009` is error-severity, so the run exits non-zero.

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
fn parse_should_return_ok_when_input_is_valid() {
    assert_eq!(parse("ok"), Ok(()));
}
```

#### TEST001 CLI output

```text
$ rust-llm-tidy --no-config --include TEST001 src/lib.rs
src/lib.rs:1: warning[TEST001]: test function `test_foo` uses a discouraged test-name pattern.

Why: Behavioral names help readers understand a test's claim without opening its body.

Suggestions:
- Rename it to describe the behavior asserted, using `subject_should_expectation`.
- Add `_when_condition` only for conditional or edge behavior. (fn `test_foo`)
```

`TEST001` is warning-severity, so the run exits 0.

### TEST002 - test missing its summary comment

Test functions must open with a short comment above their attribute block. A
`///` doc run or a plain `//` block above them satisfies it.

Detection checks presence only, never wording. A blank line between the
comment and the attributes does not count.

Before:

```rust
#[test]
fn parse_returns_ok_when_input_is_valid() {
    assert_eq!(parse("ok"), Ok(()));
}
```

After:

```rust
// Verifies the parser accepts a valid input.
#[test]
fn parse_returns_ok_when_input_is_valid() {
    assert_eq!(parse("ok"), Ok(()));
}
```

#### TEST002 CLI output

```text
$ rust-llm-tidy --no-config --all-lines --include TEST002 src/lib.rs
src/lib.rs:1: reminder[TEST002]: test function `parse_returns_ok_when_input_is_valid` is missing a short explanatory comment above its attributes.

Why: Readers need to understand why this test matters without tracing its body.

Suggestions:
- Explain the requirement, edge case, or regression the test protects against.
- Add context rather than restating the test name.
- Consider Arrange–Act–Assert: setup, action, then assertions, separated by comments. (fn `parse_returns_ok_when_input_is_valid`)
```

`TEST002` is reminder-severity, so the run exits 0 and it reports on changed
lines by default. C# reports the same rule; see [lints for C#].

#### Remarks

Rust checks top-level test functions only; functions inside inline modules,
including `#[cfg(test)] mod tests`, are not inspected.

### MOD001 - oversized module

Warn once when a code file exceeds the line budget (default 500).

- Count every physical line: blanks, comments, tests. Recognized module
  headers (leading file comments, `//!` docs, Python's module docstring)
  stay out of the count by default; item docs and body comments count.
- Rust excludes top-level `#[cfg(test)] mod` regions and `tests/` paths by
  default. Other test code counts.
- Non-code files require the [opt-in below].

Exactly 500 lines passes with the default budget; line 501 triggers the warning.
The warning points to the first counted line over budget. A final line counts
with or without a newline; a trailing newline adds no extra line.

Tune the budget through the `module_size.max_lines` config key (default
500).

#### MOD001 counting options

```yaml
module_size:
  max_lines: 500
  include_non_code: false       # include selected config, data, and prose files
  include_in_file_tests: false  # include Rust's #[cfg(test)] mod regions
  include_test_files: false     # include Rust files under tests/ directories
  exclude_module_headers: true  # keep module headers out of the count
# Other languages always count inline tests and test files.
```

#### MOD001 CLI output

```text
$ rust-llm-tidy --no-config --include MOD001 src/big.rs
src/big.rs:501: warning[MOD001]: file has 612 lines outside `#[cfg(test)]` mod regions,
over the 500-line budget (module_size.max_lines).
Why:
- Large files make readers search farther and keep more context in mind.
- Focused modules help readers find responsibilities without scanning unrelated code.
- Large files cost LLMs more input tokens when read in full and leave less
  context for other relevant code.
Suggestions:
- Consider keeping entry points and orchestration near the top level, with
  implementation details in focused child modules.
- Group code by responsibility, such as parsing or validation. Domain names
  usually explain more than catch-all names like `utils`.
- Let orchestration read as calls to clear operations, such as `parse_imports`
  or `validate_config`.
- Free functions suit stateless work. Methods suit behavior that manages a
  type's state.
- Keep closely related code together. A split need not add new types,
  forwarding wrappers, or a wider public API.
- Preserve behavior and performance across the split. Avoid needless
  allocations, clones, or repeated work just to cross module boundaries.
- Update overview docs to explain responsibilities and point readers to the
  entry points. Keep useful documentation; a split should not remove it.
- In Rust, the module root (`mod.rs` or `foo.rs`) is the usual home for
  those entry points.
- Selective re-exports can expose child-module entry points without a
  forwarding wrapper.
Counting:
- Top-level `#[cfg(test)]` test modules are excluded.
  Other lines count, including comments and blank lines.
- Rust files in `tests/` directories are skipped. (file)
```

`MOD001` is warning-severity, so the run exits 0.

#### Remarks

`ini`/`json` require explicit extension selection and gain only MOD001.

### MOD002 - function-local `use` without `#[cfg]`

Keep `use` declarations at module scope so dependencies are easy to find.

Hoist unconditional imports. Keep an import function-local only if it
needs conditional compilation, such as for platform-specific code.

- The `use` itself must carry `#[cfg]` to be exempt.
- An enclosing `#[cfg]` does not exempt the import.
- `#[cfg_attr(...)]` does not count.

Before:

```rust
fn load() {
    use std::io::Read;
}
```

After:

```rust
use std::io::Read;

fn load() {}
```

#### MOD002 CLI output

```text
$ rust-llm-tidy --no-config --include MOD002 src/lib.rs
src/lib.rs:2: error[MOD002]: function-local `use` lacks its own `#[cfg]`.
Why: readers can find module-scope imports without searching function bodies.
Suggestions:
- Move it to the containing module's imports, keeping visibility private.
  Preserve its target and alias; check for name or trait-method conflicts.
- Preserve any enclosing compilation conditions when moving it.
  Keep it local if needed, with the real `#[cfg]` condition on the `use`. (use)
Error: found 1 error(s)
```

`MOD002` is error-severity, so the run exits non-zero.

### MOD003 - full namespace qualification in code

Flags paths that include the full namespace;
partial paths are allowed at any depth.

Recognized roots:

- `crate::` and absolute `::`
- Unshadowed `std`, `core`, and `alloc`
- Visible, unconditional `extern crate` names

Unknown roots and glob-ambiguous crate names stay silent. Imports alone do not
prove a crate root.

Before (`example.rs`):

```rust
fn f() { std::sync::Arc::new(1); }
```

After:

```rust
use std::sync::Arc;

fn f() { Arc::new(1); }
```

#### MOD003 CLI output

With a `config.yml` containing `{}`, the local CLI renders:

```text
$ cargo run -p rust-llm-tidy-cli -- --config config.yml --include MOD003 example.rs
example.rs:1: hint[MOD003]: path `std::sync::Arc::new` includes the full namespace.
Why: full namespace prefixes give readers longer lines to scan before reaching the item name, making code harder to understand.
Suggestions:
- If clear at the call site, add `use std::sync::Arc;` at module scope and use `Arc::new`.
- Import a parent module if the bare name loses context: for example, import `std::process` and use `process::id()`, not `id()`.
- Keep the full path if shortening would reduce clarity or create a name conflict.
- Verify the shorter path resolves to the same item; this hint uses syntax, not compiler name resolution. (fn `f`)
```

If `use std::sync::Arc;` already exists on line 1 and the same function
sits on line 3, the first bullet is instead:

```text
$ cargo run -p rust-llm-tidy-cli -- --config config.yml --include MOD003 example.rs
example.rs:3: hint[MOD003]: path `std::sync::Arc::new` includes the full namespace.
Why: full namespace prefixes give readers longer lines to scan before reaching the item name, making code harder to understand.
Suggestions:
- If clear at the call site, use `Arc::new`; `Arc` is already imported.
- Import a parent module if the bare name loses context: for example, import `std::process` and use `process::id()`, not `id()`.
- Keep the full path if shortening would reduce clarity or create a name conflict.
- Verify the shorter path resolves to the same item; this hint uses syntax, not compiler name resolution. (fn `f`)
```

#### Remarks

Hints use existing aliases, such as `Shared::new` for `Arc as Shared`.
They never fail the run or rewrite source.

MOD003 skips:

- Imports, Rust macros, and attributes.
- Partial paths, including `self::`, `super::`, imported modules, and aliases.
- Paths whose shorter name would be ambiguous or shadowed.
- C# expression receivers not recognized as namespace or type paths.
- Code guarded by Rust `cfg`/`cfg_attr` or C# `#if`, including the whole
  function or method containing the condition.

See [C# MOD003] for C# import advice.

### LEN001 - oversized function or method

Suggests reviewing Rust functions that exceed `method_length.max_lines`
(default 100).

Counts body code lines only, excluding signatures, blank lines, and comment-only
lines.

All functions are checked, including tests and inner functions. Inner-function
lines also count toward the enclosing function.

#### LEN001 threshold options

```yaml
method_length:
  max_lines: 100  # hint above this many counted body lines; >= 1.
```

#### LEN001 CLI output

```text
$ rust-llm-tidy --no-config --include LEN001 src/merge.rs
src/merge.rs:2: hint[LEN001]: fn `merge_all` has 101 body lines (blank and comment-only lines excluded),
over the 100-line budget (method_length.max_lines).
Why:
- Long functions can make readers track too much control flow and local state.
- Named, cohesive steps can help readers follow the flow without tracking every detail.
Suggestions:
- Consider extracting cohesive steps into functions named for what they do,
  so the outer function reads as an overview of the flow.
- Keep closely related work together. Avoid new types, forwarding wrappers,
  or a wider public API solely to shorten the body.
- Preserve behavior and performance. Avoid extra allocations, cloning, or
  repeated work; measure performance-sensitive changes.
- Mark extracted functions as `#[inline]` if needed.
- Inner functions still count toward the enclosing body.
- Keep the body intact if splitting would make it harder to follow or slower. (fn `merge_all`)
```

`LEN001` is hint-severity, so the run exits 0.

### SYM - configured symbol policies

`SYM` reports text hints across supported text languages and custom or
built-in symbol hints for Rust and C#.

See [symbol rules] for matching, configuration, declaration exclusions,
and built-in symbols.

### DUP001 - same-file textual duplication

Reminds when five meaningful lines repeat at three non-overlapping sites.

Before: adding `metrics` repeats five fields, despite the different hosts.

```rust
let primary = ConnectionOptions {
    host: "primary.internal",
    port: 443,
    tls: true,
    timeout_secs: 30,
    retries: 3,
    keep_alive: true,
};
let backup = ConnectionOptions {
    host: "backup.internal",
    port: 443,
    tls: true,
    timeout_secs: 30,
    retries: 3,
    keep_alive: true,
};
let metrics = ConnectionOptions {
    host: "metrics.internal",
    port: 443,
    tls: true,
    timeout_secs: 30,
    retries: 3,
    keep_alive: true,
};
```

After: put those shared defaults in `ConnectionOptions::new(host)`.

```rust
let primary = ConnectionOptions::new("primary.internal");
let backup = ConnectionOptions::new("backup.internal");
let metrics = ConnectionOptions::new("metrics.internal");
```

#### DUP001 CLI output

```text
input.rs:19: reminder[DUP001]: 5 meaningful lines repeat at 3 non-overlapping sites in this file. Locations: 3-7, 11-15, 19-23.

Why:
- Less repeated code is easier to audit and keep consistent.

Suggestions:
- Consider a constructor or helper if it preserves behavior, contracts, and performance.
- A useful refactor may not clear this reminder. That is OK; do not force further changes just to silence it. (source sequence)
```

#### Remarks

Matches contiguous text within one changed run, not symbols.

Default matching ignores edge whitespace. See the [example config] for
thresholds and exact mode.

No automatic fix.

[example config]: ../.rust-llm-tidy.example.yml

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
    "message": "non-private item is missing a doc comment.\n\nWhy: Readers need its purpose and contract without tracing the implementation.\n\nSuggestions:\n- Add `///` docs describing its purpose and actual contract, based on the implementation and relevant callers.",
    "item_kind": "fn",
    "item_name": "load",
    "title": "missing documentation"
  }
]
```

Fields:

- `severity` - `"error"`, `"warning"`, `"hint"`, or `"reminder"` for findings,
  `"success"` for change records (applied or would-be changes)
- `line` - 1-based reported line; `null` when the record has no
  specific line (e.g. link/table fixes)
- `item_name` - item name, `null` when unnamed
- `title` - friendly title for lint findings,
  `null` for change records
- `path`, `code`, `message`, `item_kind` - as in plaintext

In JSON mode the plaintext `path:line: sev[CODE]: ...` diagnostics are not
printed to stderr. Change records and lint findings are folded into the same
document, in both in-place and `--dry-run` runs.

## Hints and reminders

`hint` is an advisory severity for suggestions such as shorter wording.

- Hints never gate the exit code; only `error` findings do.
- Text mode prints them in a separate group at the end, in the usual
  `path:line: hint[CODE]: ...` shape.
- JSON mode records them with `severity: "hint"` and the usual lint fields.

Reminders are hints scoped to [changed lines] by default.
They report as `reminder` in text and JSON output.

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
[`DOC009`]: #doc009---module-file-without-top-level-docs
[`TEXT001`]: ./text-lints.md#text001---oversized-paragraph
[`TEXT002`]: ./text-lints.md#text002---long-line
[`TEXT003`]: ./text-lints.md#text003---long-sentence
[`TEXT004`]: ./text-lints.md#text004---header-opener-shape
[`TEXT005`]: ./text-lints.md#text005---fenced-code-block-without-a-language-tag
[`TEXT006`]: ./text-lints.md#text006---verbose-synonyms
[`TEXT007`]: ./text-lints.md#text007---passive-voice-and-past-behaviour
[`TEXT008`]: ./text-lints.md#text008---dense-bullet-list
[`TEST001`]: #test001---non-behavioral-test-name
[`TEST002`]: #test002---test-missing-its-summary-comment
[`MOD001`]: #mod001---oversized-module
[`MOD002`]: #mod002---function-local-use-without-cfg
[`MOD003`]: #mod003---full-namespace-qualification-in-code
[C# MOD003]: languages/lints/csharp.md#mod003---full-namespace-qualification-in-code
[`LEN001`]: #len001---oversized-function-or-method
[`SYM`]: #sym---configured-symbol-policies
[`DUP001`]: #dup001---same-file-textual-duplication
[lints for C#]: ./languages/lints/csharp.md
[text lints]: ./text-lints.md

## Library access

Use `rust_llm_tidy::rules::lint`.
For complete processing and project context, see [library entry points].

[library entry points]: architecture.md#library-entry-points
[opt-in below]: #mod001-counting-options
[changed lines]: #reporting-scope
[symbol rules]: symbol-rules.md
