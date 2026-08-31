# rust-llm-tidy

[![Crates.io](https://img.shields.io/crates/v/rust-llm-tidy-cli.svg)][Crates.io CLI]
[![CI](https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg)][CI]

Opinionated linter for my Rust and Markdown preferences. Primarily for
fixing LLM output, but useful for any code.

> **⚠ Largely written by LLMs - personal tooling, not production software.**
>
> Just a disclaimer.
> 
> Although I review each line, and patch by hand as I go.
> This isn't done to the same quality standard as my other projects.

## Quick Start

### Install

Pick either install method:

```bash
# From crates.io (builds from source)
cargo install rust-llm-tidy-cli

# From a GitHub Release (prebuilt binaries)
# Download the matching <platform>.7z from the latest release
# (e.g. linux-x64.7z, windows-x64.7z, macos-*.7z) and extract to a
# directory on your PATH.
```

`rust-llm-tidy` is a single command that runs the full pipeline (fix,
reorder, vis, lints) by default on `.rs` and `.md` files:

```bash
# Tidy a file in place
rust-llm-tidy src/main.rs

# Preview the changes that would be made without writing them
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

Global flags: `--dry-run`, `--config <PATH>`, `--no-config`, `--validate`,
`--include <OP>` (repeatable), `--exclude <OP>` (repeatable),
`--output-mode <MODE>` (`text` default or `json`), `--json` (alias for
`--output-mode json`).

#### Operations

| Op        | Does                                    | Mutates | Default on? |
| --------- | --------------------------------------- | ------- | ----------- |
| `tables`  | align GFM tables                        | yes     | yes         |
| `fences`  | alternate nested fence delimiters       | yes     | yes         |
| `links`   | hoist inline links to reference style   | yes     | yes         |
| `reorder` | review-friendly item order              | yes     | yes         |
| `vis`     | narrow bare `pub` in restricted modules | yes     | yes         |
| `lints`   | run DOC001-DOC008 + TEST001 checks      | no      | yes         |

#### JSON output

Emit every lint finding and change record as one JSON array on stdout, in both
in-place and `--dry-run` runs. Prints `[]` when there are no findings or
changes, and still prints the document when the run exits non-zero:

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

- `severity` - `"error"` or `"warning"` for lint findings, `"success"` for
  change records (applied or would-be changes)
- `line` - 1-based item start line; `null` when the record has no specific
  line (e.g. link/table fixes)
- `item_name` - item name, `null` when unnamed
- `path`, `code`, `message`, `item_kind` - as in plaintext

In JSON mode the per-line plaintext diagnostics are not printed to stderr.
Change records and lint findings are folded into the same document.
`--dry-run` previews the changes without writing them.
See [change reporting] for the shared record
format.

#### Feature documentation

| Feature                        | Page              |
| ------------------------------ | ----------------- |
| Reorder                        | [docs/reorder.md] |
| Visibility narrowing           | [docs/vis.md]     |
| Table alignment                | [docs/tables.md]  |
| Fence delimiters               | [docs/fences.md]  |
| Link hoisting                  | [docs/links.md]   |
| Lints (DOC001-DOC008, TEST001) | [docs/lints.md]   |

### Configuration

`rust-llm-tidy` reads an optional YAML config file named
`.rust-llm-tidy.yml`. It supports three rule-selection keys:

- `include` - whitelist mode: for matched paths, run ONLY these rules.
  A file matching NO `include` group runs nothing.
- `exclude` - blacklist mode: for matched paths, never run these rules.
- `exclude_files` - skip ALL processing for matched files.

`include` and `exclude` are mutually exclusive (both non-empty -> config-load
error). Omitting `paths` in an `include` or `exclude` group implies `["**"]`
(all paths).

Additionally:

- `post_process` - external commands run on processed files after tidying.
  Each entry has `command`, `args` (default empty), and `extensions`
  (default empty = all). File path appended as last arg. Skipped under `--dry-run`
  and for excluded files.

Globs are relative to the config file's directory and use `literal_separator`
semantics (`*` does not cross `/`; `**` recurses). Rule names are the nine
lint codes (DOC001-DOC008, TEST001) plus the six ops: `tables`, `fences`,
`links`, `reorder`, `vis`, `lints`.

The config is discovered automatically: `rust-llm-tidy` walks up from the
current directory to the repo root (the first ancestor containing `.git`),
checking each level for `.rust-llm-tidy.yml`; the first found wins. Override
with `--config <PATH>`; disable all discovery and loading with `--no-config`.

Every config error - bad YAML, unknown rule name, bad glob syntax, or a pattern
matching zero files - produces a non-zero exit on every command. This is so LLMs
can immediately detect config errors. There is no "warn and continue" mode.
Use `--validate` to check the config without processing files (useful in CI).

```bash
# Validate the config without processing files
rust-llm-tidy --validate

# Run with an explicit config path
rust-llm-tidy --config ./my-config.yml src

# Disable config discovery for an ad-hoc run
rust-llm-tidy --no-config --include tables README.md
```

#### Opt-in example

[`.rust-llm-tidy.example.yml`] shows whitelist
configuration with feature levels. It is not auto-discovered. Copy it to
`.rust-llm-tidy.yml`, or pass it explicitly with `--config`.

#### Rule updates

Consider pinning `rust-llm-tidy` in CI for reproducible checks. When you want to
update or enable new rules, upgrade the pinned version deliberately. This avoids
unexpected lint failures after a tool update.

## Crates

- [rust-llm-tidy-model]: Shared source model - parse top-level items with
  spans, visibility, and doc comments; atomic I/O; line-multiset safety
- [rust-llm-tidy-reorder]: Reordering library - reference-graph ordering of
  items into a review-friendly 10-phase order
- [rust-llm-tidy-vis]: Visibility narrowing - narrows bare `pub` items inside
  restricted-visibility inline modules to the module's visibility
- [rust-llm-tidy-lint]: Linter - missing-docs, incomplete-`# Errors`/
  `# Arguments`, placeholder, and test-naming checks
- [rust-llm-tidy-fix]: Markdown/doc-comment tidying - realigns GFM tables,
  fixes nested fence delimiters, and hoists inline links to reference style
  in `.rs` doc comments and `.md` files
- [cli]: Unified command-line interface (fix, reorder, vis, lints)

## Git Hook (optional)

Auto-tidy staged `.rs` files on commit (fix + reorder + vis + lints +
rustfmt). One-time setup per clone:

```bash
git config core.hooksPath .githooks
```

`pre-commit` is a dispatcher that runs every executable `*.sh` in
`.githooks/pre-commit.d/` in lexicographic order - name files with a number
prefix (`10-`, `20-`, ...) to fix the run order. Add hooks by dropping files
there. Works on Linux, macOS, and Windows (Git for Windows).

Bundled hooks, in run order:

- `10-tidy-rust.sh` - runs `rust-llm-tidy` on staged `.rs` files (fix,
  reorder, vis, lints), then `rustfmt` via the config `post_process` step. Uses
  an installed `rust-llm-tidy` binary if present, else `cargo run`.

## GitHub Action

[`rust-llm-tidy-action`] runs `rust-llm-tidy` in CI on Rust and Markdown
files to gate pull requests. Full input reference in the [action README].

Minimal usage (default `mode: apply`; on a PR commits and pushes fixes):

```yaml
- name: Tidy
  uses: Sewer56/rust-llm-tidy-action@v1
  with:
    rust-project-path: '.'
    changed-files: true   # only files this PR changed
```

## Developer Manual

For step-by-step development guidance, see the [Developer Manual].

## Contributing

We welcome contributions! See the [Contributing Guide] for details.

## License

Licensed under [Apache 2.0].

[Crates.io]: https://crates.io/crates/rust-llm-tidy-reorder
[Docs.rs]: https://docs.rs/rust-llm-tidy-reorder
[Crates.io CLI]: https://crates.io/crates/rust-llm-tidy-cli
[Docs.rs CLI]: https://docs.rs/rust-llm-tidy-cli

[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[rust-llm-tidy-model]: ./src/rust-llm-tidy-model/README.MD
[rust-llm-tidy-reorder]: ./src/rust-llm-tidy-reorder/README.MD

[rust-llm-tidy-vis]: ./src/rust-llm-tidy-vis/README.MD
[rust-llm-tidy-lint]: ./src/rust-llm-tidy-lint/README.MD
[rust-llm-tidy-fix]: ./src/rust-llm-tidy-fix/README.MD
[cli]: ./src/cli/README.MD

[action README]: ./submodules/action/README.MD
[`rust-llm-tidy-action`]: https://github.com/Sewer56/rust-llm-tidy-action
[Developer Manual]: https://reloaded-project.github.io/reloaded-templates-rust/manual/

[Contributing Guide]: https://reloaded-project.github.io/reloaded-templates-rust/manual/#contributing
[Apache 2.0]: ./LICENSE
[change reporting]: ./docs/lints.md#change-reporting
[docs/reorder.md]: ./docs/reorder.md

[docs/vis.md]: ./docs/vis.md
[docs/tables.md]: ./docs/tables.md
[docs/fences.md]: ./docs/fences.md
[docs/links.md]: ./docs/links.md
[docs/lints.md]: ./docs/lints.md

[`.rust-llm-tidy.example.yml`]: ./.rust-llm-tidy.example.yml
