# rust-llm-tidy-cli

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-cli.svg)][Crates.io]
[![Docs.rs](https://docs.rs/rust-llm-tidy-cli/badge.svg)][Docs.rs]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Command-line interface for [rust-llm-tidy-reorder].

<!-- 
    This README is displayed on crates.io & docs.rs
    Write detailed documentation here for users.
-->

## Installation

```bash
cargo install rust-llm-tidy-cli
```

Or build from source:

```bash
cargo build --release -p rust-llm-tidy-cli
```

## Usage

`rust-llm-tidy` is a single command that runs the full pipeline (fix,
reorder, vis, lints) by default on `.rs` and `.md` files:

```bash
# Tidy a file in place
rust-llm-tidy src/main.rs

# Report the changes that would be made instead of modifying the file
rust-llm-tidy --dry-run src/main.rs

# Tidy every .rs/.md file under a directory recursively
rust-llm-tidy src

# No paths given -> process the changed files in the current git diff
rust-llm-tidy

# Run only specific ops (repeatable); overrides the config `include` mode
rust-llm-tidy --include vis src/lib.rs
rust-llm-tidy --include reorder --include vis src

# Skip an op for this run (additive to config `exclude`)
rust-llm-tidy --exclude lints src

# Validate the config without processing files
rust-llm-tidy --validate

# Emit the lint findings as a single JSON array on stdout
rust-llm-tidy --output-mode json src/main.rs
```

### Flags

| Flag                   | Effect                                                          |
| ---------------------- | --------------------------------------------------------------- |
| `--dry-run`            | Print the changes that would be made instead of modifying files |
| `--config <PATH>`      | Explicit config path (overrides auto-discovery)                 |
| `--no-config`          | Disable config discovery and loading                            |
| `--validate`           | Validate config and exit (no files processed)                   |
| `--include <OP>`       | Run only these ops/lint-codes (repeatable)                      |
| `--exclude <OP>`       | Skip these ops/lint-codes (repeatable)                          |
| `--output-mode <MODE>` | Lint output format: `text` (default) or `json`                  |
| `--json`               | Alias for `--output-mode json`                                  |

### JSON output

Print every lint finding and change record as one JSON array on stdout instead
of the default `path:line: sev[CODE]: ...` stderr diagnostics. Change records
are emitted for both in-place and `--dry-run` runs. Prints `[]` when there are
no findings or changes, even when the run exits non-zero:

```json
[
  {
    "path": "src/main.rs",
    "line": 22,
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
- `line` - 1-based item start line
- `item_name` - item name, `null` when unnamed
- `path`, `code`, `message`, `item_kind` - as in plaintext

In JSON mode the plaintext per-line diagnostics are not printed to stderr.
`--output-mode json` combines freely with `--dry-run`; the would-be changes are
folded into the same document. See
[dry-run change reporting](../../docs/lints.md#change-reporting) for
the shared record format.

### Operations

| Op        | Does                                    | Mutates | Default on? |
| --------- | --------------------------------------- | ------- | ----------- |
| `tables`  | align GFM tables                        | yes     | yes         |
| `fences`  | alternate nested fence delimiters       | yes     | yes         |
| `links`   | hoist inline links to reference style   | yes     | yes         |
| `reorder` | canonical item ordering                 | yes     | yes         |
| `vis`     | narrow bare `pub` in restricted modules | yes     | yes         |
| `lints`   | run DOC001-DOC006 + TEST001 checks      | no      | yes         |

An optional YAML config file (`.rust-llm-tidy.yml`) may exclude files from
processing, whitelist or blacklist specific rules per path, and run external
programs (e.g. `rustfmt`) on every processed file. Pass `--config <PATH>` to
use an explicit config or `--no-config` to disable discovery. See the
repository README for the full schema.

## Ordering Rules

The algorithm orders top-level items across phases. Within most phases, an item
comes before any item it references; alphabetical order breaks ties. See
[docs/reorder.md](../../docs/reorder.md) for an annotated 10-phase example.

## License

Licensed under Apache 2.0

[Crates.io]: https://crates.io/crates/rust-llm-tidy-cli
[Docs.rs]: https://docs.rs/rust-llm-tidy-cli
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[rust-llm-tidy-reorder]: ../rust-llm-tidy-reorder/README.MD
