# Symbol rules

Configure text hints, Rust/C# symbol hints, and declaration exclusions
with `symbol_rules`.

The first matching custom hint wins per occurrence. Exclusions apply
independently of list order.

```yaml
symbol_rules:                                     # Ordered custom hints and declaration exclusions.
  # Literal invocation matching
  - symbol: Vec::new                              # Literal suffix; use :: in Rust and C#.
    title: Initial capacity                       # Required hint heading.
    message: "Review the initial capacity."       # Required diagnostic body.
    # languages: [rust]                           # Restrict literal/declaration matching.
    # extensions: [rs]                            # Without dots; intersects languages when set.
    # target: usage                               # Default; declaration matches named declarations.
    # action: hint                                # Default; exclude requires target: declaration.
    # zero_arguments: true                        # false requires arguments; omitted allows either.
    # severity: reminder                          # Default; also hint, warning, error, ai_reminder.
    # scope: changed_lines                        # Or all; --all-lines takes precedence.

  # C# array matching
  - symbol: new[]                                 # Synthetic C# array name.
    title: Array initialization                   # Required hint heading.
    message: "Review array initialization."       # Required diagnostic body.
    # array_kind: explicit_sized_vector           # One sized rank, non-array element type.
    # no_initializer: true                        # false requires an initializer; omitted allows either.

  # Raw text matching, including strings
  - regex: 'TODO'                                  # Usage mode searches text without implicit anchors.
    title: Unfinished work                         # Required hint heading.
    message: "Review this unfinished work marker." # Required diagnostic body.
    # extensions: [rs, cs, py]                     # Omitted searches all processed text languages.
    # comments: include                            # Default: all text; exclude: outside; only: comments.

  # Declaration protection
  - regex: 'internal::.*'                         # Declaration mode matches the whole normalized name.
    target: declaration                           # Required for exclusions.
    action: exclude                               # Protect matching declarations from edits and lints.
    # languages: [rust]                           # Omitted allows Rust and C#.
```

## Symbol matching

Set exactly one of `symbol` or `regex` per entry.

`symbol` matches a case-sensitive component suffix: `Vec::new` also matches
`std::vec::Vec::new`, but not `OtherVec::new`.

C# configuration also uses `::`, not dots: `symbol: Console::WriteLine`
matches `Console.WriteLine("hello")`.

- Symbol paths use `::` without generic arguments.
- Rust methods expose only the method name; macros retain `!`.
- Explicit C# object creations append `::new` to the written type path.
  Target-typed `new()` is not a named object usage.
- Matching uses written syntax, not resolved imports, aliases, overloads,
  receiver types, or macro expansions. Comments, strings, and non-invoked
  references are not usages.

## Regex matching

Usage `regex` searches text in any supported text language.

This includes strings and non-invoked names, without implicit anchors.
Matching is case-sensitive; use inline flags such as `(?i)`, `(?m)`, or `(?s)`
for case, line anchors, or matching across newlines.

`comments` selects where usage regexes search:

- `include` (default): all text, without requiring comment parsing.
- `exclude`: text outside recognized comments.
- `only`: each recognized comment, including its delimiters.

For `exclude` and `only`, matches cannot cross comment boundaries. Anchors
apply to each searched region.

Unavailable or uncertain comment recognition skips these rules with a warning;
`include` rules still run.

Each regex produces non-overlapping matches. At the same starting byte,
the first matching regex rule wins. Symbol matches are independent. Findings
use the match's first line. If the run changes the text, matching uses the
updated text.

## Declaration regex matching

Use `target: declaration` to match a named function, method, or type rather
than its uses. The regex must match its full name, including its containers.

For example, this function's name is `internal::load`:

```rust
mod internal {
    fn load() {}
}
```

This C# method's name is `Internal::Store::Load`:

```csharp
namespace Internal {
    class Store {
        void Load() {}
    }
}
```

This rule excludes declarations inside the Rust module:

```yaml
symbol_rules:
  - regex: 'internal::.*'
    target: declaration
    action: exclude
```

Use `Internal::.*` for the C# namespace instead. Matching is case-sensitive.
Declaration rules do not accept `comments`.

## Fields

- `languages`: `rust` and/or `csharp` for parsed-name matching; omitted enables
  both. Not allowed on usage regexes: use `extensions` instead.
- `extensions`: case-insensitive extensions without dots. Usage regexes accept
  any supported text-lint extension; omission covers all processed text languages.
- `target`: `usage` (default) or `declaration`.
- `action`: `hint` (default) or `exclude`.

Parsed-name extensions accept `rs` and `cs` and intersect `languages`.
Selectors do not expand file discovery. Empty or unsupported lists fail loading.

### Hint output

Hints report a title and message with configurable severity and scope.

- `title` and `message`: required nonblank text for hints. The title precedes
  the verbatim message in text output. `Why:` and `Suggestions:` headings
  in messages are recommended, not required.
- `severity`: `reminder` (default), `hint`, `warning`, `error`, or
  `ai_reminder` for AI-only guidance. Exclusions ignore it. `action: hint`
  does not imply `severity: hint`.
- `scope`: `all` or `changed_lines`, available for every severity.
  Exclusions do not use a reporting scope.

Scope chooses whole-file or changed-line reporting. Only errors fail the run.
`reminder` and `ai_reminder` default to changed lines; other severities default
to whole files. See [reporting scope] for overrides.

### Usage constraints

- `zero_arguments`: `true` requires no call arguments; `false` requires some.
- `no_initializer`: `true` requires no initializer; `false` requires one.
- `array_kind`: `any` or `explicit_sized_vector`, for C# arrays only.

Omit a constraint to allow either form. These fields apply only to literal
usage rules, not declaration rules or regexes.

For `symbol: build` with `zero_arguments: true`:

```rust
build();  // Matches: no arguments.
build(8); // Does not match: has an argument.
```

For `symbol: new[]`, `array_kind: explicit_sized_vector`, and
`no_initializer: true`:

```csharp
var empty = new byte[8];      // Matches: one sized rank, no initializer.
var filled = new byte[] { 1 }; // No match: initializer present.
var matrix = new byte[2, 4];  // No match: multiple dimensions.
```

`array_kind: any` allows all C# array creations, including `new[] { 1 }`.
Collection expressions and `stackalloc` are not array creations here.
Array sizes are not call arguments; array findings point to `new`.

## Declaration exclusions

Use `target: declaration` with `action: exclude` to skip lints, built-in edits,
or external post-processing.

Keep reordering and other edits enabled while suppressing `DOC001`:

```yaml
symbol_rules:
  - regex: 'internal::.*'
    target: declaration
    action: exclude
    exclude_lints: [DOC001]
    exclude_edits: false
```

- `exclude_lints`: `true` suppresses all declaration-local lints; a list
  suppresses selected lint codes. `false` or `[]` suppresses none.
- `exclude_edits`: `true` protects matching code and its docs from built-in
  edits; `false` permits edits. Defaults to `true`, as does `exclude_lints`.
- `exclude_post_process`: `true` skips external post-processing for the whole
  file when a declaration matches. Defaults to `false`.

All three controls are independent and forbidden on hints. Overlapping rules
add exclusions; `false` does not undo another rule.

- Omit `title`, `message`, `scope`, and usage constraints.
- Exclusions remain active with `SYM` or all lints disabled, regardless
  of reporting scope.

File-level checks not specific to a symbol still run. External tools may edit
declarations even with `exclude_edits: true` unless `exclude_post_process: true`
opts the file out.
Syntax errors fail processing when declaration policies apply.

## Built-in symbols

Built-in families provide reminders mainly to improve LLM output, scoped to
changed lines by default.

- `PERF001` suggests reviewing initial capacity for Rust and C# containers.
  See the [Rust capacity definitions] and [C# capacity definitions].
- `PERF002` suggests reviewing zero initialization for C# `new T[length]`
  without an initializer. See the [array definition].

Select families with the `perf_hints` code list:

- Omitted or `[PERF001, PERF002]`: enable both (default).
- `[PERF001]` or `[PERF002]`: enable only that family.
- `[]`: disable both. Custom `symbol_rules` remain active.
- Unknown codes fail configuration loading; repeated codes do not duplicate
  findings.

Both families report `SYM` at `reminder` severity.
Use `SYM` in lint selection and `lint_scopes`, not the family
codes. A built-in and a custom hint can both report at the same occurrence.

## Configuration errors

Unknown fields and enum values fail configuration loading.
Compilation errors identify the one-based `symbol_rules` entry to fix.

- Supply exactly one nonblank matcher and valid fields for the action and
  target. Patterns need not match an existing file.
- Correct invalid regex syntax or simplify patterns rejected by the regex
  dependency's default compilation limits.

Configuration is trusted. There are no application limits on rule counts or
text lengths; regex compilation retains the dependency defaults.

Validate the configuration:

```bash
cargo run -p rust-llm-tidy-cli -- --config config.yml --validate
```

[reporting scope]: lints.md#reporting-scope
[Rust capacity definitions]: ../src/rust-llm-tidy/src/rules/lint/symbols/builtins/rust_capacity.rs
[C# capacity definitions]: ../src/rust-llm-tidy/src/rules/lint/symbols/builtins/csharp_capacity.rs
[array definition]: ../src/rust-llm-tidy/src/rules/lint/symbols/builtins.rs
