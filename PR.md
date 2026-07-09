# Port to tree-sitter

Migrates parsing from `syn` to `tree-sitter` (+ `tree-sitter-rust`) in `model`, `reorder`, `vis`. Removes `syn`, `proc-macro2` from workspace. Prerequisite for future language-agnostic `LangSpec` extraction (out of scope).

- Base `main` ← branch `tree-sitter`; commits `c75775f`, `3a4f985`
- `SourceItem` seam unchanged; all tests pass byte-exact; `.cargo/verify.sh` green (build+benches, test, clippy, rustdoc, fmt)

## Changes

**model `parse/`** — `parse_source` drives tree-sitter; `classify_item` switches on CST node kinds. `PendingTrivia` attaches preceding `#[...]`/`///` to each item. Gap-anchored byte spans preserved; line/col conversion dropped (tree-sitter yields byte offsets). `ParseResult` holds `Tree`, exposed via `syntax_tree()` (was `syntax_file()`).

**reorder `graph/`** — `ReferenceCollector` rewritten as recursive CST walk: pushes named item kinds, records edges for path/type identifiers in reference position (decls skipped), reverses local-macro edges. `toposort`: removed dead `extract_bare_fn_name`.

**vis** — new `ParsedFile { path, source, tree }`; `build_module_tree`/`collect_crate_reexports` take `&[ParsedFile]` so each file parses once. Visibility via `visibility_modifier.named_child_count()` (0=bare `pub`, ≥1=restricted); byte spans from `start_byte`/`end_byte`. `proc_macro2` span hack + `line_start_offsets` gone. `#[path]`/`#[cfg]` as preceding-sibling `attribute_item`; `use` via `scoped_identifier`/`use_as_clause`/`use_wildcard`/`scoped_use_list`/`use_list`.
- **Fix**: `{self}` re-export now re-exports path's last segment (prior syn keyed no-op `self`). New test locks it.
- `cargo_metadata` retained (Cargo-root discovery orthogonal). README updated for `ParsedFile`.

**cli + benches** — switched to `ParsedFile` API; `force()`/`force_span_fallback()` dropped.

## Deps

- Removed: `syn`, `proc-macro2`, `memchr` (vis only).
- Added: `tree-sitter = "0.26"`, `tree-sitter-rust = "0.24"`.
- `syn` → `tree-sitter` in crate `keywords`.

## Breaking API

- `syntax_file()` → `syntax_tree()` (returns `&Tree`).
- `build_module_tree`/`collect_crate_reexports` now take `&[ParsedFile]`.
- `toposort::extract_bare_fn_name` removed.

## Follow-up

- **Semver**: crates at `0.1.0`, API breaks. CI `semver-checks` may need `0.2.0` bump (not in this PR).
- **Cargo.lock**: orphaned `syn`/`proc-macro2` entries left; full `cargo update` deferred (churn risk).
