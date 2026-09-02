# Languages - tiers, defaults, and known effects

`rust-llm-tidy` admits source files by extension. Every admitted
extension resolves to a profile that decides which ops may run for it and
which line-comment prefixes the fix ops strip around tables and fences.

Extension matching is case-insensitive everywhere, so `.MD` resolves like
`.md`.

## Tiers

| Tier            | Extensions                              | Prefixes     | Ops on by default               |
| --------------- | --------------------------------------- | ------------ | ------------------------------- |
| Markdown family | `md`, `markdown`, `txt`, `text`, `mdx`  | `///`, `//!` | tables, fences, links, lints    |
| Rust            | `rs`                                    | `///`, `//!` | every op                        |
| C#              | `cs`                                    | `///`, `//`  | tables, reorder, lints          |
| Code languages  | the 41 extensions in the families below | per family   | tables                          |
| Unmapped        | any other extension                     | none         | tables                          |
| Data formats    | `ini`, `json`, `toml`, `yaml`, `yml`    | none         | none; never admitted by default |

Notes on the op columns:

- `lints` splits by tier: Rust runs all nine codes (DOC001-DOC008,
  TEST001); the markdown family runs the two text checks (DOC007,
  DOC008); C# runs DOC001-DOC006 and TEST001 against XML doc comments
  (see [lints for C#]).
- Code languages and C# can additionally run `fences`, but only through
  an explicit `--include fences` or a config include. No other op turns
  on for them.
- `reorder` for C# follows its own ordering profile; see
  [reorder for C#].

## Comment-marker families

Tables inside line comments realign with the language's comment marker
stripped and re-applied, so every row keeps its marker and indent.

| Marker | Extensions                                                                                                         |
| ------ | ------------------------------------------------------------------------------------------------------------------ |
| `//`   | `c`, `cc`, `cpp`, `h`, `hpp`, `java`, `js`, `mjs`, `ts`, `tsx`, `go`, `swift`, `kt`, `php`, `dart`, `scala`, `zig` |
| `#`    | `bash`, `conf`, `jl`, `nim`, `pl`, `py`, `pyi`, `r`, `rb`, `sh`, `zsh`                                             |
| `--`   | `ada`, `elm`, `hs`, `lua`, `sql`                                                                                   |
| `;`    | `clj`, `cljc`, `el`, `lisp`, `scm`                                                                                 |
| `%`    | `erl`, `m`, `tex`                                                                                                  |

C# also uses `//` comments but sits in its own tier because it carries
AST ops.

## Why code languages stop at tables

- `links` never runs outside the markdown family and Rust. An appended
  definition line is invalid syntax in other languages, and the inline
  form is indistinguishable from indexing followed by a call:

  ```text
  a[i](x)
  ```

- `fences` stays off by default for code languages: without a parser,
  comment and string literals are indistinguishable, so a fence-looking
  line inside a string could be flipped.
- `reorder`, `vis`, and the parser-driven lint checks need a registered
  language backend. Backends today: Rust and C#.

## Admission override and additions

- The config `extensions:` key replaces the default admitted list when
  non-empty: only the listed extensions run. An empty or absent list keeps
  the defaults.
- The config `extra_extensions:` key and `--extension <EXT>` (repeatable)
  admit extensions in addition to the effective list
  (`extensions:` when non-empty, else the defaults).
- Values are written without the leading dot and matched
  case-insensitively. Malformed entries (leading or inner dot, path
  separator, whitespace) fail the run.
- A listed extension with no registry entry resolves to the unmapped
  tier: tables only, no comment prefixes.
- The data formats (`ini`, `json`, `toml`, `yaml`, `yml`) admit no ops
  even when listed explicitly.
- `exclude_files` globs skip matched files entirely.

## Known effects

- String-literal re-pad: a GFM-shaped table inside a string literal of a
  code file is whitespace-re-padded like any table row. Cell contents
  are untouched, so semantics survive; the bytes change.
- mdx: JSX interacts with markdown parsing edge cases; the markdown
  family keeps full ops anyway.
- org-mode and AsciiDoc tables deliberately fail the GFM delimiter
  check, so those files stay byte-unchanged even when admitted.

[lints for C#]: ./lints/csharp.md
[reorder for C#]: ./reorder/csharp.md
