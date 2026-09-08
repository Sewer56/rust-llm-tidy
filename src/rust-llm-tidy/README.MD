# rust-llm-tidy

Lint and transform source with the same library used by the `rust-llm-tidy` CLI.

> Personal tooling, primarily for correcting LLM output. Review changes before
applying them to important source.

## Installation

```toml
[dependencies]
rust-llm-tidy = "0.1.0"
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

Source processing returns final transformed text and findings against that text.

- Project-dependent checks use standalone facts
- Use `run` for cross-file analysis

`Severity::Reminder` defaults to changed-line reporting, unlike `Hint`.
Standalone buffers have no input diff, so reminders are hidden by default.
Set `SourceOptions::lint_scope` to `Some(ReportingScope::All)` for an audit;
import `ReportingScope` from `rust_llm_tidy::config`.

### File processing

`RunOptions::default()` previews explicit inputs. It does not discover
configuration or query Git automatically.

- Set `paths` to files or directories
- Set `apply` to permit file writes
- Set `git_changed` to query tracked Git changes when paths are empty
- Set `cargo_discovery` to permit Cargo-based Rust project discovery
- Set `post_process` to permit configured subprocesses during an apply run
- Use `config::load_and_compile` to supply configuration explicitly
- Inspect `RunReport::ensure_success` after consuming partial results

`RunOptions::lint_scope` overrides every lint's reporting scope.
`RunOptions::diff_base` grants baseline Git reads and compares against the
merge-base of that local reference and `HEAD`.

With `git_changed` alone,
the reporting baseline is `HEAD` versus working content.

Changed-line findings need a usable snapshot. Missing Git permission or
context produces a warning and skips those findings, not an all-lines
fallback. An invalid explicit baseline is an error.

Compiled config supplies `symbol_rules` and `lint_scopes` to `run`.
See [symbol policies] for matching, declaration protection, array rules,
scope precedence, and conservative post-transform line remapping.

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
[symbol policies]: ../../docs/lints.md#sym001---configured-symbol-policies
[architecture and migration]: ../../docs/architecture.md
[Apache 2.0]: ../../LICENSE
