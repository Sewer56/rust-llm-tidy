After changes, find and run `.llm/verify.{sh,ps1}` to test + lint.
Print all output.
Use the local CLI, not global `rust-llm-tidy`.
Invoke via `cargo run -p rust-llm-tidy-cli`.

Performance:

- Prefer borrows: `&str` / `&[T]` returns, `&'static str` constants,
  `Cow<'_, str>` for conditional ownership.
- `Box<str>` for immutable strings.
- Reuse buffers via `.clear()`.
- Const generics for compile-time branching such as
  `<const LINE_NUMBERS: bool>`.
- Prefer performance-oriented crates such as `parking_lot` and `memchr`.
- Keep dependency footprint minimal.
