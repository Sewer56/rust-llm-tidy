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
