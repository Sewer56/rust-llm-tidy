# `vis` - narrow bare `pub` in restricted-visibility modules

## What it does

Items inside a limited-visibility module (`pub(crate) mod`, `pub(super) mod`,
...) written as `pub` get narrowed to match the module: `pub(crate) mod` items
become `pub(crate)`, `pub(super)` become `pub(super)`.

Exception: names re-exported (`pub use`) keep `pub`. They're public API.

By default it sees the whole crate - reads the module tree and re-exports once,
applies to each file. Files outside `src/` (tests, benches, fixtures) work
alone, checking only their own re-exports.

```rust,ignore
// Before
pub(crate) mod m {
    pub fn f() {}
}

// After
pub(crate) mod m {
    pub(crate) fn f() {}
}
```

## Config

```yaml
exclude:
  - paths: ["src/exports/**/*.rs"]
    rules: [vis]
```

```bash
rust-llm-tidy --include vis --dry-run src/lib.rs
rust-llm-tidy --exclude vis src
```

## Change output

Every run reports each item it narrows (or would narrow under `--dry-run`) as
one record. In text mode every record prints to stderr:

```text
src/lib.rs:3: success[VIS]: narrow visibility of `f` at line 3 (fn `f`)
```

In JSON mode the records appear on stdout with `severity: "success"`:

```json
[
  {
    "path": "src/lib.rs",
    "line": 3,
    "severity": "success",
    "code": "VIS",
    "message": "narrow visibility of `f` at line 3",
    "item_kind": "fn",
    "item_name": "f"
  }
]
```

See [Change reporting](./lints.md#change-reporting) for the shared format.
