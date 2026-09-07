# rust-llm-tidy

[![Crates.io][crates-badge]][Crates.io CLI]
[![CI][ci-badge]][CI]

Opinionated linter for my Rust, C#, and Markdown preferences.
Primarily for fixing LLM output, but useful for any code.

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
reorder, vis, lints) by default on every allowed source file.

Which files run and which ops they get is decided by the language
registry; data formats like `.json` are skipped.
```bash
# Tidy a file in place
rust-llm-tidy src/main.rs

# Preview the changes that would be made without writing them
rust-llm-tidy --dry-run src/main.rs

# Tidy every allowed source file under a directory recursively
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
`--include <OP>`, `--exclude <OP>`, `--extension <EXT>` (all repeatable), and
`--output-mode text|json` (`--json` for short).

#### Operations

| Op        | Does                                    | Languages         | Mutates |
| --------- | --------------------------------------- | ----------------- | ------- |
| `tables`  | align GFM tables                        | all allowed       | yes     |
| `fences`  | alternate nested fence delimiters       | all allowed       | yes     |
| `links`   | hoist inline links to reference style   | `md` family, `rs` | yes     |
| `reorder` | review-friendly item order              | `rs`, `cs`        | yes     |
| `vis`     | narrow bare `pub` in restricted modules | `rs`              | yes     |
| `lints`   | run DOC*, TEXT*, TEST* checks           | all allowed       | no      |

Every allowed op is on by default; `--include`/`--exclude` and the config
narrow them. The `md` family is `.md`, `.markdown`, `.txt`, `.text`,
`.mdx`.

`lints` splits by language: `rs` and `cs` run every code; every other
allowed language runs the text lints only.

#### JSON output

Emit every lint finding and change record as one JSON array on stdout, in both
in-place and `--dry-run` runs.

- Prints `[]` when there are no findings or changes
- Still prints the document when the run exits non-zero:

```json
[
  {
    "path": "src/main.rs",
    "line": 22,
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
- `line` - 1-based item start line; `null` when the record has no specific
  line (e.g. link/table fixes)
- `item_name` - item name, `null` when unnamed
- `title` - friendly per-code title for lint findings, `null` for change
  records
- `path`, `code`, `message`, `item_kind` - as in plaintext

In JSON mode the per-line plaintext diagnostics are not printed to stderr.
Change records and lint findings are folded into the same document.
`--dry-run` previews the changes without writing them.

See [change reporting] for the shared record format.

#### Feature documentation

| Feature              | Page                 |
| -------------------- | -------------------- |
| Reorder              | [docs/reorder.md]    |
| Visibility narrowing | [docs/vis.md]        |
| Table alignment      | [docs/tables.md]     |
| Fence delimiters     | [docs/fences.md]     |
| Link hoisting        | [docs/links.md]      |
| Lints                | [docs/lints.md]      |
| Text lints           | [docs/text-lints.md] |

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

- `extensions` - replaces the default allowed extensions when non-empty;
  empty or absent keeps the defaults.
- `extra_extensions` - added on top of the effective list (`extensions`
  when non-empty, else the defaults). Both lists take dotless entries,
  matched case-insensitively.
- `post_process` - external commands run on processed files after tidying.
  Each entry has `command`, `args` (default empty), and `extensions`
  (default empty = all). File path appended as last arg.
  - Skipped under `--dry-run` and for excluded files.

Globs are relative to the config file's directory and use `literal_separator`
semantics (`*` does not cross `/`; `**` recurses).

Rule names are the lint codes plus the six ops: `tables`, `fences`,
`links`, `reorder`, `vis`, `lints`.

The config is discovered automatically: `rust-llm-tidy` walks up from the
current directory to the repo root (the first ancestor containing `.git`),
checking each level for `.rust-llm-tidy.yml`; the first found wins.

Override with `--config <PATH>`; disable all discovery and loading with
`--no-config`.

Every config error produces a non-zero exit on every command. That covers
bad YAML, unknown rule name, bad glob syntax, a malformed
`extensions`/`extra_extensions` entry, or a pattern matching zero files.

This is so LLMs can immediately detect config errors. There is no "warn and
continue" mode. Use `--validate` to check the config without processing files
(useful in CI).

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
configuration with feature levels.

- It is not auto-discovered.
- Copy it to `.rust-llm-tidy.yml`, or pass it explicitly with `--config`.

#### Rule updates

Consider pinning `rust-llm-tidy` in CI for reproducible checks.

- When you want to update or enable new rules, upgrade the pinned version
  deliberately.
- This avoids unexpected lint failures after a tool update.

## Library and CLI

The workspace contains two crates:

- [rust-llm-tidy]: everything else
- [cli]: argument parsing, output rendering and exit status

All lint and transform rules live in the library's `rules/` tree.
The CLI calls the same public entry points as library consumers.

Library entry points:

- `tidy_source`: process in-memory source without filesystem or subprocess
  access
- `run`: process selected files with explicit write, Git-selection and
  subprocess permissions

See [architecture and migration] for module ownership, preview semantics and
retired crate imports.

## GitHub Action

[`rust-llm-tidy-action`] runs `rust-llm-tidy` in CI on every allowed
source file to gate pull requests. Full input reference in the [action
README].

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

[Crates.io]: https://crates.io/crates/rust-llm-tidy
[Docs.rs]: https://docs.rs/rust-llm-tidy
[Crates.io CLI]: https://crates.io/crates/rust-llm-tidy-cli
[Docs.rs CLI]: https://docs.rs/rust-llm-tidy-cli
[CI]: https://github.com/Sewer56/rust-llm-tidy/actions
[crates-badge]: https://img.shields.io/crates/v/rust-llm-tidy-cli.svg
[ci-badge]: https://github.com/Sewer56/rust-llm-tidy/actions/workflows/rust.yml/badge.svg
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
[docs/text-lints.md]: ./docs/text-lints.md
[`.rust-llm-tidy.example.yml`]: ./.rust-llm-tidy.example.yml

[rust-llm-tidy]: ./src/rust-llm-tidy/README.MD
[architecture and migration]: ./docs/architecture.md
