# `vis` - narrow bare `pub` in restricted-visibility modules

## What it does

Narrows bare `pub` items declared inside a restricted-visibility inline
module (`pub(crate) mod`, `pub(super) mod`, ...) down to the module's
visibility. Crate-aware by default: when a crate root is discovered, the
module tree floor and the crate-wide re-export set are computed once and
applied per file. Files outside the crate `src/` tree (integration tests,
benches, fixtures) narrow standalone with a per-file re-export guard.

Re-exported names keep `pub` (they are part of the crate's public API); the
guard prevents narrowing an item that is `pub use`-d elsewhere.

## Before

```rust,ignore
pub(crate) mod m {
    pub fn f() {}
}
```

## After

```rust,ignore
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
