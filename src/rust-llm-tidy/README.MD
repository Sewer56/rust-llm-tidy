# rust-llm-tidy

Lint and transform source with the same library used by the `rust-llm-tidy` CLI.

> Personal tooling, primarily for correcting LLM output. Review changes before
applying them to important source.

## Installation

```toml
[dependencies]
rust-llm-tidy = "0.4.1"
```

## Entry points

- `tidy_source` processes an in-memory buffer without filesystem or subprocess
  access
- `run` processes selected files with configuration, project analysis and
  explicit execution permissions
- `rules` exposes lint and transformation operations for lower-level composition
- `reporting` contains diagnostics, changes and complete run results

The library never prints results or exits the process. Its caller controls
presentation and failure handling.

### Standalone source

```rust
use rust_llm_tidy::{SourceOptions, tidy_source};

let options = SourceOptions {
    include: vec!["DOC001".into()],
    ..SourceOptions::default()
};
let result = tidy_source("pub fn load() {}\n", "rs", &options)?;
assert_eq!(result.diagnostics[0].code, "DOC001");
# Ok::<(), anyhow::Error>(())
```

`tidy_source` returns the transformed text and findings against it, using only
facts available in the buffer. Use `run` when checks need cross-file analysis.

Set `SourceOptions::symbol_rules` for custom hints and declaration exclusions.
Exclusions can suppress selected lints while allowing edits such as reordering.

Buffers have no Git diff, so reminders are hidden by default. To include them,
set `SourceOptions::all_lines` to `true`. This overrides reporting scopes for
every severity.

Hints, warnings, and errors still cover the whole buffer.

Enabled DUP001 warns and skips buffer analysis unless `all_lines` is true.

### File processing

Pass files or directories in `RunOptions::paths` and optional configuration from
`config::load_and_compile` to `run`.

The defaults preview those inputs without
discovering configuration, querying Git, or starting subprocesses.

- Set `apply` to permit file writes
- Set `git_changed` to query tracked Git changes when paths are empty
- Set `cargo_discovery` to permit Cargo-based Rust project discovery
- Set `post_process` to permit configured subprocesses during an apply run

`RunOptions::all_lines: true` overrides symbol-entry and `lint_scopes` settings
for every severity. The default, `false`, respects configured scopes.
Reminders default to changed lines; hints, warnings, and errors default to all.

`diff_base` permits Git reads against the merge-base of a local reference and
`HEAD`. With no paths, it also discovers eligible files below the current
directory. With `git_changed` alone, the baseline is `HEAD` versus working
content.

Changed-line findings need a usable snapshot. Missing Git permission or
context produces a warning and skips those findings, not an all-lines
fallback. An invalid explicit baseline is an error.

Consume the returned results before calling `RunReport::ensure_success` to
check for errors, including failures that left partial results.

See [symbol policies] for matching and declaration protection, and
[reporting scope] for scope precedence and baseline comparison.

See the [API documentation] for executable examples and error contracts.

## Organization

All rule implementations live under one source tree:

- `rules/lint/{rust,csharp,text}` contains diagnostic policies
- `rules/transform/{tables,fences,links,reorder,visibility}` contains source
  transformations
- `rules/registry.rs` owns stable rule codes and titles
- `languages` owns parsing, language profiles and syntax facts
- `text` owns comment extraction and documentation measurement
- `pipeline`, `project`, `input` and `config` implement reusable execution

See [architecture and migration] for the retired crate mappings.

## License

Licensed under [Apache 2.0].

[API documentation]: https://docs.rs/rust-llm-tidy
[symbol policies]: ../../docs/symbol-rules.md
[reporting scope]: ../../docs/lints.md#reporting-scope
[architecture and migration]: ../../docs/architecture.md
[Apache 2.0]: ../../LICENSE
