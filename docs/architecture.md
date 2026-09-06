# Library architecture and migration

The workspace has one processing library and one command-line adapter.

```text
rust-llm-tidy-cli -> rust-llm-tidy
```

## Ownership

- Library: source transformations, lint policies, language parsing and project
  analysis
- Library: configuration, input selection, file execution and structured reports
- CLI: arguments, help, plaintext/JSON rendering and process exit status

All lint and transformation implementations live in
`src/rust-llm-tidy/src/rules/`.
Language-specific syntax facts remain under `languages/`, outside the rule
policy modules.

## Library entry points

### Source buffers

`tidy_source(source, extension, options)` runs without filesystem or subprocess
access.

- Returns final text, changes and diagnostics
- Borrows the input when final text is unchanged
- Applies transformations before collecting lint findings
- Uses standalone Rust visibility and C# throw facts

Use explicit rule selection for a lint-only call. Language defaults also enable
supported transformations.

### Files and projects

`run(options, config)` provides the same processing used by the CLI.

- Expands selected paths and applies extension and configuration policy
- Coordinates Rust visibility context and C# project-reference analysis
- Preserves operation order, line-preservation checks and atomic writes
- Returns ordered file results, project warnings and subprocess failures

Permissions are explicit through `RunOptions`:

- `apply`: permits writes; false by default
- `git_changed`: permits Git selection for empty paths; false by default
- `cargo_discovery`: permits Cargo project discovery; false by default
- `post_process`: permits configured subprocesses during apply; false by default

Configuration discovery and loading are explicit calls in `config`.
Loading configuration alone never executes its post-processing commands.

Without `cargo_discovery`, Rust visibility uses standalone facts.
The CLI explicitly enables Cargo discovery.
Excluded visibility does not trigger discovery.

### Preview semantics

File preview preserves the CLI's existing behavior: each operation reads the
unchanged file on disk.
Its lint findings therefore describe the original file, not a simulated final
transformation.

The buffer API returns transformed source and lints that final source instead.
These are intentionally different entry-point contracts, not equivalent preview
implementations.

### Failure handling

Setup failures return `Err`, before a complete run report is available.
Individual file and subprocess failures remain in `RunReport` alongside
successful results.

Consume the report before calling `ensure_success` when partial results matter.
Warnings and hints alone do not make `ensure_success` fail.

## Migrating library imports

Replace the old library dependencies with `rust-llm-tidy`.
The retired packages are no longer workspace members or release targets.

| Previous import                                                   | New import                                                              |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `rust_llm_tidy_model::parse`                                      | `rust_llm_tidy::source`                                                 |
| `rust_llm_tidy_model::io`                                         | `rust_llm_tidy::input::file_io`                                         |
| `rust_llm_tidy_model::safety`                                     | `rust_llm_tidy::source::preservation`                                   |
| `rust_llm_tidy_model::line_endings`                               | `rust_llm_tidy::source::line_endings`                                   |
| `rust_llm_tidy_lang::{LanguageBackend, RustBackend, backend_for}` | `rust_llm_tidy::languages::{LanguageBackend, RustBackend, backend_for}` |
| `rust_llm_tidy_lang::backends::CanThrowIndex`                     | `rust_llm_tidy::languages::CanThrowIndex`                               |
| `rust_llm_tidy_lang::backends::rust::visibility`                  | `rust_llm_tidy::rules::transform::visibility::rust`                     |
| `rust_llm_tidy_lang::lexicon`                                     | `rust_llm_tidy::text::comments`                                         |
| `rust_llm_tidy_lint::{Diagnostic, Severity}`                      | `rust_llm_tidy::reporting::{Diagnostic, Severity}`                      |
| `rust_llm_tidy_lint::check`                                       | `rust_llm_tidy::rules::lint`                                            |
| `rust_llm_tidy_reorder::graph`                                    | `rust_llm_tidy::rules::transform::reorder::graph`                       |
| `rust_llm_tidy_reorder::reorder`                                  | `rust_llm_tidy::rules::transform::reorder`                              |
| `rust_llm_tidy_fix`                                               | `rust_llm_tidy::rules::transform`                                       |

Prefer `tidy_source` or `run` when composing the complete product.
They apply language gating and reporting that lower-level functions do not
provide automatically.

CLI commands, configuration keys, lint codes and JSON fields remain unchanged.

## Development and releases

- Run `src/.llm/verify.sh` for workspace checks
- Run `cargo test --workspace --all-features` for library and CLI coverage
- Publish `rust-llm-tidy` before `rust-llm-tidy-cli`

All operation benchmarks and fixtures live under `src/rust-llm-tidy/benches/`.
Run them with `cargo bench -p rust-llm-tidy`.

Library implementation tests remain beside their modules.
CLI integration tests retain command behavior coverage, including library/CLI
result equivalence.

Historical plan artifacts retain their original paths as decision history.
