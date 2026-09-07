# Text lints - the TEXT* family

The text lints are `lints` sub-checks ([`lints`]) run on raw text:
comment-stripped for programming languages, raw for markdown and text
files.

Leading whitespace (a docstring's common indentation), the comment
marker run, and one following space strip before measuring; each source
keeps its own paragraphs.

## Sources per tier

| Tier                    | Measured doc text                                                                                       |
| ----------------------- | ------------------------------------------------------------------------------------------------------- |
| Markdown family         | the whole file's prose                                                                                  |
| Rust                    | `//`, `///`, `//!` line comments, outer `/** */` block docs, `#[doc = "..."]` values (like `///` lines) |
| C#                      | `///` XML doc comments: text-node inner text                                                            |
| Python                  | module, class, and function docstrings; `#` comments                                                    |
| Comment-marker families | line comments and the family's block forms (below)                                                      |

Block forms measured per family:

- `//` family and sql: `/** */` and `/* */`.
- lua: `--[[ ]]`.
- hs and elm: `{- -}`.
- el, lisp, scm: `#| |#`.
- m: `%{ %}` alone on its line.

Never measured:

- Rust: plain `/* */` comments and the inner `/*! */` and
  `#![doc = "..."]` forms.
- Python: triple-quoted strings that are not docstrings.
- The marker families: string content, heredoc payload, and code lines.
- Block docs: `*` continuations and `@tag` name tokens
  (`@param name`).
- Python `>>>` doctest examples: source lines, `...` continuations,
  and expected output, until the blank line ending the example.

Python's producer and the marker families' comment lexicon fail closed:
a file they cannot attribute safely produces no findings rather than
guesses.

## TEXT001 - oversized paragraph

A paragraph of doc text over 240 chars is an error.

- A bullet over 240 chars warns instead and recommends one checkable action
  of at most 160 chars.
- Nested bullets are separate paragraphs.

Code blocks, tables, headings, signature lines, and link definitions are
exempt as whole lines and end a paragraph.

Before:

```rust
/// Loads the configured data from disk, parses it, validates every field
/// against the schema, resolves relative paths against the config
/// directory, retries transient failures with bounded backoff, and logs a
/// one-line summary when the load settles.
pub fn load() {}
```

After:

```rust
/// Loads the configured data from disk and parses it.
///
/// - Validates every field against the schema.
/// - Resolves relative paths against the config directory.
/// - Retries transient failures with bounded backoff.
/// - Logs a one-line summary when the load settles.
pub fn load() {}
```

### TEXT001 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT001 src/lib.rs
src/lib.rs:1: error[TEXT001]: paragraph is 243 chars long.
  - Paragraphs over 240 chars outlast a short attention span.
  - Split it at the nearest idea change with a blank line.
  - Convert list-like paragraphs into bullets.
  - Keep each bullet to one checkable action of at most 160 chars.
  - Move remarks into their own sections.
  - The check skips code blocks, tables, headings, signature lines, and link definitions. (file)
Error: found 1 error(s)
```

`TEXT001` is error-severity for prose, so the run exits non-zero; bullets
exit 0.

## TEXT002 - long line

A doc line over 80 chars is a warning. Lines count in full: code spans,
URLs, and link targets included.

Code blocks, table rows, and link reference definitions are exempt.

Before:

```md
The config is discovered by walking up from the current directory to the repo root, checking each level.
```

After:

```md
The config is discovered by walking up from the current directory to the repo
root, checking each level.
```

### TEXT002 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT002 README.md
README.md:1: warning[TEXT002]: line is 104 chars long.
  - Lines over 80 chars strain short attention spans and need wide monitors.
  - Split it at the nearest idea change with a blank line.
  - Code spans, URLs, and link targets count.
  - Code blocks, table rows, and link definitions are exempt. (file)
```

`TEXT002` is warning-severity, so the run exits 0.

## TEXT003 - long sentence

A sentence over 25 words in measured doc prose is a warning.

Sentences split at `.`, `!`, and `?`; a word is a whitespace-separated
token.

Code blocks, tables, headings, signature lines, and link definitions
are exempt upstream, as for TEXT001 and TEXT002.

Before:

```rust
/// Loads the configured data from disk, parses it, and validates every field
/// against the schema before the loader resolves relative paths and retries
/// transient failures with a bounded backoff policy.
pub fn load() {}
```

After:

```rust
/// Loads the configured data from disk and parses it.
///
/// - Validates every field against the schema.
/// - Resolves relative paths and retries transient failures with bounded
///   backoff.
pub fn load() {}
```

The split is naive: decimals like `3.5` or abbreviations like `e.g.`
only shorten fragments, so misses are possible but fabricated reports
are not.

### TEXT003 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT003 src/lib.rs
src/lib.rs:1: warning[TEXT003]: sentence is 30 words long.
  - Keep sentences to 25 words or fewer.
  - Long sentences can be harder to understand.
  - Readers understand over 90% of the text when sentences contain 14 words or fewer.
  - At 43 words per sentence, comprehension drops below 10%.
  - Split this sentence where the idea changes. (file)
```

`TEXT003` is warning-severity, so the run exits 0.

## TEXT004 - header opener shape

An opener paragraph with three or more sentences, or a plain opener over
160 measured chars, is a warning.

Openers are:

- The first measured paragraph of each doc region: the file, module, or
  item opener (first paragraph of a function, method, struct, enum, or
  field doc).
- The first measured paragraph after each heading line.
- For consecutive heading lines, only the paragraph after the last
  heading; a heading with no following paragraph is never checked.

Bullet openers check sentence count only; TEXT001 owns non-opener
paragraphs.

A sentence boundary is `.`, `!`, or `?` followed by whitespace.

`e.g.` before a lowercase word, decimals, and URLs never split, so
misses are possible but fabricated reports are not.

Before:

```rust
/// Loads the configured data from disk and parses it into the schema,
/// resolving relative paths against the configured base directory for
/// every caller in the process.
pub fn load() {}
```

After:

```rust
/// Loads the configured data from disk and parses it.
///
/// - Resolves relative paths against the config directory.
/// - Serves every caller in the process.
pub fn load() {}
```

### TEXT004 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT004 src/lib.rs
src/lib.rs:1: warning[TEXT004]: opener paragraph is 161 chars long; maximum is 160.
  - Keep the opener brief so readers can find the main point quickly.
  - Lead with the main point, ideally in one short sentence.
  - Keep a plain opener to 160 measured chars or fewer.
  - Move supporting details below the opener without losing necessary information.
  - Use bullets for distinct facts, one fact per bullet.
  - Keep a connected explanation in a separate short paragraph. (file)
```

`TEXT004` is warning-severity, so the run exits 0.

## TEXT005 - fenced code block without a language tag

A markdown code fence without a language identifier warns at its opening
line.

- Bare fences and bare `ignore` warn.
- Closing fences, indented code blocks, and real tags (`ignore,foo`,
  `Ignore`) never fire.

Before:

````md
~~~
cargo build
~~~

~~~ignore
cargo test
~~~
````

After:

````md
~~~text
cargo build
~~~

~~~text,ignore
cargo test
~~~
````

### TEXT005 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT005 README.md
README.md:1: warning[TEXT005]: fenced code block has no language tag.
  - Tag the fence with its language, like ```text.
  - Untagged blocks get no syntax highlighting. (file)
README.md:5: warning[TEXT005]: fenced code block uses bare `ignore`.
  - Name the language: ```rust,ignore hides but still tags.
  - Bare `ignore` drops syntax highlighting and tooling. (file)
```

`TEXT005` is warning-severity, so the run exits 0.

## TEXT006 - verbose synonyms

A measured doc line can receive a shorter-wording hint.

- Words: `utilize` → `use`, `approximately` → `about`, `commence` → `start`
- Phrases: `due to the fact that` → `because`, `in order to` → `to`
- Redundancies: `each and every` → `each`, `absolutely essential` → `essential`
- Framing: `it is worth noting that` → omit the opener and state the point
- Context-sensitive terms: alternatives explain when technical wording
  may be needed

The [wording dictionary] contains the full list and each entry's guidance.

Before:

```text
We utilize this helper in order to show the value.
```

After:

```text
We use this helper to show the value.
```

TEXT006 never edits files. `Before` and `After` show lowercase
dictionary guidance, not rewritten lines. Check meaning and grammar before use.

Matching rules:

- Words: ASCII case-insensitive; listed forms only; alphanumeric/`_` boundaries.
- Phrases: whitespace only between words; no punctuation, code, or line breaks.
- Selection: first match per line; longest phrase wins ties.
- Exemptions: inline code and fenced or indented code blocks.

### TEXT006 CLI output

```text
$ cargo run -p rust-llm-tidy-cli -- --include TEXT006 src/lib.rs
src/lib.rs:3: hint[TEXT006]: consider simpler wording.
  - Before: `utilize`
  - After: `use`
  - Preserve meaning and adjust grammar to fit. (file)
```

`TEXT006` has hint severity, so its findings do not fail the run.

[wording dictionary]: ../src/rust-llm-tidy/src/rules/lint/text/text006_verbose_synonyms/suggestions.rs

## TEXT007 - passive voice and past behaviour

Warns when docs may use passive voice or describe past behaviour.
Say what the code does, directly:

- Passive: `Errors are returned by the scanner.` →
  `The scanner returns errors.`
- Past behaviour: `This no longer panics.` →
  `This returns an error.` (if accurate)
- Time labels: `The parser currently rejects empty names.` →
  `The parser rejects empty names.`

Describe only current behaviour in active, present-tense language.
Preserve conditions and guarantees rather than comparing old and new behaviour.

### Detection details

- Checks each line; reports at most one warning, preferring passive voice.
- Flags be-verbs followed by past participles, but allows state descriptions
  such as `is required` and `is deprecated`.
- Flags phrases: `no longer`, `used to`, `in the past`.
- Flags words: `previously`, `now`, `formerly`, `historically`, `originally`,
  `recently`, `lately`, `currently`, `anymore`, and bare `was`.
- Flags clause-initial `Before,`.
  Allows temporal uses such as `before validation`.
- Matches alphabetic words case-insensitively, not substrings within words.
- May flag harmless phrases such as `previously refuted findings` or
  `recently accessed entries`. This heuristic does not infer grammatical context.
- By default, allows past-behaviour wording in `CHANGELOG*` or `MIGRATION*`
  basenames at any depth and files under a `releases` directory.
  Matching is case-insensitive; passive voice still warns.

Before:

```rust
/// Errors are returned by the scanner.
pub fn scan() {}
```

After:

```rust
/// The scanner returns errors.
pub fn scan() {}
```

### Release-note suppression

Set this top-level boolean in `.rust-llm-tidy.yml` to report narration
markers in release and migration notes too:

```yaml
suppress_in_release_notes: false
```

- Omitted or `true`: keep narration-marker suppression in those paths
- `false`: report narration markers there, just as in ordinary files
- Passive-voice checks and rule inclusion/exclusion: unchanged

This setting has no CLI flag. File processing applies it; pathless text
checks do not apply release-note suppression.

### TEXT007 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT007 src/lib.rs
src/lib.rs:1: warning[TEXT007]: passive construction: `are returned`.
  - State only current behavior in active, present-tense language.
  - Remove change history, old/new comparisons, and time labels such as `now` or `currently`.
  - Delete history-only sentences; do not invent replacement behavior.
  - Check the implementation before rewriting; preserve exact conditions, guarantees, and limitations.
  - Keep history out of comments and API docs, including internals, tests, and helpers. Use release or migration notes only for a genuine public-API compatibility concern. (file)
```

`TEXT007` is warning-severity, so the run exits 0.

[`lints`]: ./lints.md

## Library access

Use `rust_llm_tidy::rules::lint::{run_text_checks, run_region_checks}`.
For complete processing and project context, see [library entry points].

[library entry points]: architecture.md#library-entry-points
