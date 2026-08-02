# `lints` - documentation and test-naming checks

## What it does

The `lints` op runs seven read-only checks (DOC001-DOC006 + TEST001). It is
on by default in the pipeline and never mutates files. Exits non-zero when
any error-severity finding is present (warnings do not fail).

The seven lint codes are sub-checks of `lints`; they stay individually
toggleable through the same rule namespace as the ops, so
`exclude: [{rules: [DOC001]}]` turns off just missing-docs and
`exclude: [{rules: [lints]}]` turns off all linting.

## Codes

| Code | Severity | Fires when | Example |
| ---- | -------- | ---------- | ------- |
| `DOC001` | Error | A non-private item has no `///` doc comment. | `pub fn f() {}` |
| `DOC002` | Error | A `pub fn` returning `Result` has no `# Errors` section. | `pub fn load() -> Result<(), E> { ... }` |
| `DOC003` | Warning | A `# Errors` section names no concrete error variant. | `# Errors\n\nReturns an error if it fails.` |
| `DOC004` | Warning | A `pub fn` with parameters has no `# Arguments` section. | `pub fn greet(name: &str) {}` |
| `DOC005` | Warning | A `# Arguments` section does not mention every parameter name. | `# Arguments\n\n` * `name` … (omits `fmt`) |
| `DOC006` | Warning | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`/...). | `/// TODO: finish this` |
| `TEST001` | Warning | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. | `#[test] fn test_foo() {}` |

## Per-code fixtures

One fixture per code lives under `src/cli/tests/fixtures/doc/`
(`doc001_missing_docs.rs`, `doc002_missing_errors_section.rs`,
`doc003_vague_errors.rs`, `doc004_missing_arguments.rs`,
`doc005_undocumented_param.rs`, `doc006_placeholders.rs`,
`test001_test_naming.rs`, plus `clean.rs` as the negative case).

## Config

```yaml
# Turn off all linting
exclude:
  - rules: [lints]            # paths omitted -> implied ["**"]

# Turn off just missing-docs
exclude:
  - rules: [DOC001]
```

```bash
# Run only linting for this invocation
rust-llm-tidy --include lints src
# Skip linting for this invocation
rust-llm-tidy --exclude lints src
```
