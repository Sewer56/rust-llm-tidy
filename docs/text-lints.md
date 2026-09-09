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

The comment-marker families include `.yaml`, `.yml`, `.toml`, `.ps1`,
`.graphql`, `.fish`, `.cmake`, `.applescript`, and `.v`.

YAML and TOML run comment lints only. Table and fence fixes remain
disabled, even when explicitly selected, to preserve configuration values.

Reuse mappings such as `.bzl`, `.ksh`, `.vhd`, `.purs`, `.sty`, and
`.scss` share the existing lexicons' markers.

Block forms measured per family:

- `//` family and sql: `/** */` and `/* */`.
- lua: `--[[ ]]`.
- hs and elm: `{- -}`.
- el, lisp, scm: `#| |#`.
- m: `%{ %}` alone on its line.
- PowerShell (`ps1`, `psm1`, `psd1`): `<# #>`.
- AppleScript: `(* *)`.

Never measured:

- Rust: plain `/* */` comments and the inner `/*! */` and
  `#![doc = "..."]` forms.
- Python: triple-quoted strings that are not docstrings.
- The marker families: string content, heredoc payload, and code lines.
- Block docs: `*` continuations and `@tag` name tokens
  (`@param name`).
- Python `>>>` doctest examples: source lines, `...` continuations,
  and expected output, until the blank line ending the example.

Fail-closed cases: a file the producer or lexicon cannot attribute
safely produces no findings rather than guesses.

- Python: broken syntax (for example an unterminated string) yields no
  tree, so no findings.
- Marker families: unmodeled literal forms reject the scan: an
  unterminated `sh` heredoc, a Ruby `%w[...]`, a nested `js` template
  hole.
- YAML block scalars (`|` and `>` headers, including anchors and tags):
  the whole scan rejects, so a file using one produces no findings.
- CMake bracket arguments and bracket comments: the whole scan rejects.
- PowerShell here-strings and interpolated `$()` subexpressions: the
  whole scan rejects. Ordinary quoted strings use PowerShell escape rules.

## TEXT001 - oversized paragraph

A paragraph of doc text over 240 chars is an error.

- A bullet over 240 chars warns instead and recommends one fact or checkable
  action, ideally at most 160 chars.
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
Why: Dense paragraphs make it harder to find an idea and keep its context in mind.
Suggestions:
  - Split it at the nearest idea change with a blank line.
  - Keep each paragraph to 240 chars or fewer.
  - Convert list-like paragraphs into bullets.
  - Aim for one fact or checkable action per bullet, ideally at most 160 chars.
  - Move supporting remarks into their own sections; preserve necessary information, contracts, and code identifiers.
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
Why: Long lines are harder to follow in narrow editors and side-by-side reviews.
Suggestions:
  - Wrap prose at word boundaries to 80 chars or fewer per line.
  - Preserve paragraph and list structure; do not split code identifiers, code spans, or URLs.
  - Code spans, URLs, and link targets count.
  - Code blocks, table rows, and link definitions are exempt.
  - Borders are ignored. (file)
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

### TEXT003 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT003 src/lib.rs
src/lib.rs:1: warning[TEXT003]: sentence is 30 words long.
Why:
  - Long sentences can be harder to understand.
  - According to a study, readers understand about 90% at 14 words per sentence.
  - It reports under 10% at 43 words.
Suggestions:
  - Split this sentence where the idea changes.
  - Keep sentences to 25 words or fewer.
  - Aim for 14 words when the meaning allows.
  - Preserve meaning, conditions, and guarantees. (file)
```

`TEXT003` is warning-severity, so the run exits 0.

### Remarks

The split is naive: decimals like `3.5` or abbreviations like `e.g.`
only shorten fragments, so misses are possible but fabricated reports
are not.

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
src/lib.rs:1: warning[TEXT004]: opener paragraph is 162 chars long; maximum is 160.
Why: A long opener delays the main point and makes the section harder to scan.
Suggestions:
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
- Closing fences, indented code blocks, and other info strings (including
  `rust,ignore`, `ignore,foo`, and `Ignore`) never fire.

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
~~~sh
cargo build
~~~

~~~sh
cargo test
~~~
````

### TEXT005 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT005 README.md
README.md:1: warning[TEXT005]: fenced code block has no language tag.
Why:
  - Language tags give readers language cues and useful syntax highlighting.
  - Tested examples help readers apply them correctly.
Suggestions:
  - Prefer compilable Rust examples tagged ```rust and tested with doctests.
  - Use ```rust,ignore only when a doctest genuinely cannot compile or run; explain why.
  - Tag other languages accurately, such as ```sh; reserve ```text for plain text. (file)
README.md:5: warning[TEXT005]: fenced code block uses bare `ignore`.
Why:
  - Language tags give readers language cues and useful syntax highlighting.
  - Tested examples help readers apply them correctly.
  - Bare `ignore` skips Rust doctest compilation and execution.
Suggestions:
  - Prefer compilable Rust examples: replace `ignore` with `rust` and pass doctests.
  - Use ```rust,ignore only when a doctest genuinely cannot compile or run; explain why.
  - Tag other languages accurately, such as ```sh; reserve ```text for plain text. (file)
```

`TEXT005` is warning-severity, so the run exits 0.

### Remarks

TEXT005 checks fence tags, not whether examples compile or pass tests.

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
$ rust-llm-tidy --no-config --include TEXT006 README.md
README.md:1: hint[TEXT006]: wording has a simpler alternative: `utilize`.
Why: Unnecessary formal wording and framing can make the point harder to understand.
Suggestions:
  - Before: `utilize`
  - After: `use`
  - Preserve meaning and adjust grammar to fit.
  - Use the alternative only if it preserves technical meaning, uncertainty, and required wording. (file)
```

`TEXT006` has hint severity, so its findings do not fail the run.

[wording dictionary]: ../src/rust-llm-tidy/src/rules/lint/text/text006_verbose_synonyms/suggestions.rs
[module]: ../src/rust-llm-tidy/src/rules/lint/text/text007_passive_narration/mod.rs

## TEXT007 - passive voice and past behaviour

Suggests checking docs for passive voice or implementation history.
Say what the code does, directly:

- Passive: `Errors are returned by the scanner.` →
  `The scanner returns errors.`
- Past behaviour: `This no longer panics.` →
  `This returns an error.` (if accurate)

### Detection details

- Checks each line; reports at most one reminder, preferring passive voice.
- Flags be-verbs followed by past participles, but allows state descriptions
  such as `is required` and `is deprecated`.
- Flags history wording such as `no longer`, `previously`, and `prior to
  this change`; allows temporal uses such as `before validation`.

By default, allows past-behaviour wording in `CHANGELOG*` or `MIGRATION*`
basenames at any depth and files under a `releases` directory.
Matching is case-insensitive; passive voice still produces reminders.

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

### Enablement and reporting scope

TEXT007 runs by default at Reminder severity on changed lines.

Use `--all-lines` to audit unchanged lines too, or `lint_scopes: {TEXT007: all}`
to configure that scope. Disable it with:

```yaml
passive_narration:
  enable: false
```

- `enable` omitted or `true`: enable TEXT007
- `enable: false`: disable TEXT007
- Explicit `TEXT007` inclusion overrides this opt-out, but not exclusions
- The `lints` group respects the opt-out; inclusion does not override scope
- Low-level library text checks return unfiltered reminders
- `tidy_source` has no diff: set `SourceOptions::all_lines` to report reminders

File library calls require `RunOptions::git_changed` or `diff_base` for
changed-line reporting. Without an available baseline, reminders stay hidden;
`--all-lines` overrides scope without enabling disabled rules.

### Release-note suppression

Set this boolean under `passive_narration` to report narration
markers in release and migration notes too:

```yaml
passive_narration:
  enable: true
  suppress_in_release_notes: false
```

- Omitted or `true`: keep narration-marker suppression in those paths
- `false`: report narration markers there, just as in ordinary files
- Passive-voice checks and rule inclusion/exclusion: unchanged

### TEXT007 CLI output

```text
$ cargo run -p rust-llm-tidy-cli -- --include TEXT007 --all-lines src/lib.rs
src/lib.rs:1: reminder[TEXT007]: passive construction: `are returned`.
Why: Passive actions can obscure who does what. Readers usually need current behavior, not implementation history.
Suggestions:
  - Check the implementation before rewriting; this heuristic can flag valid state descriptions and runtime history.
  - For passive actions, name the actor and action when known; do not invent an actor or change the meaning.
  - For implementation history, state verified current behavior in present tense; remove old/new comparisons and redundant `now` or `currently` labels.
  - Delete sentences only when they contain implementation history alone; do not invent replacement behavior.
  - Preserve valid state descriptions, runtime history, exact conditions, guarantees, limitations, and code identifiers.
  - Keep implementation history out of comments and API docs, including internals, tests, and helpers. Use release or migration notes only for a genuine public-API compatibility concern. (file)
```

`TEXT007` emits reminders; reminders alone exit 0.

### Remarks

This heuristic has no grammatical context: expect false positives and
treat every reminder as a review suggestion, never a rewrite.

Edge-case exceptions are listed in the rule's module documentation:
[`text007_passive_narration/mod.rs`][module]

## TEXT008 - dense bullet list

An unordered list exceeding 10 source lines warns once at its first line.

- Count unordered bullets, including nested bullets and wrapped lines.
- Ignore blank lines and tables without resetting the count.
- Reset at prose, `#` headings, code blocks, or numbered lists.
- Exempt numbered lists.

Before:

```md
- Configuration paths now resolve relative to the config file instead of
  the working directory, so scheduled jobs use the same inputs as local runs.
- Unknown configuration fields report their names and source locations,
  helping users correct typos before a deployment starts.
- Connection retries use bounded backoff and stop after the configured
  deadline, preventing unavailable services from blocking shutdown.
- Failed uploads retain their checkpoints so the next attempt resumes
  from the last confirmed chunk instead of retransmitting the entire file.
- Progress output includes completed bytes and the remaining file count,
  making stalled transfers easier to distinguish from slow connections.
- Error reports include the affected file and a suggested recovery action,
  so operators can resolve failures without searching debug logs.
```

After:

```md
### Configuration

Catch setup mistakes before deployment.

- Resolve paths relative to the config file for consistent scheduled runs.
- Report unknown fields with source locations to help correct typos.

### Transfers

Recover from interruptions without blocking shutdown.

- Retry connections with bounded backoff until the configured deadline.
- Resume failed uploads from the last confirmed chunk.

### Diagnostics

Give operators enough context to identify and resolve failures.

- Show completed bytes and remaining files to distinguish slow transfers.
- Include the affected file and a recovery action in error reports.
```

### TEXT008 CLI output

```text
$ cargo run -p rust-llm-tidy-cli -- --include TEXT008 README.md
README.md:1: warning[TEXT008]: bullet list spans more than 10 lines.
Why: Long bullet lists make related facts harder to locate, especially when items wrap across lines.
Suggestions:
  - Group related bullets under subheadings; use a table only for comparable fields.
  - Shorten wording where possible; do not join wrapped lines just to meet the budget.
  - Preserve necessary information, contracts, code identifiers, and list hierarchy. (file)
```

`TEXT008` is warning-severity, so its findings do not fail the run.

[`lints`]: ./lints.md

## TEXT009 - forbidden characters

Reject selected characters in documentation prose with custom rewrite guidance.

Em dashes (`U+2014`) are forbidden by default. Findings are errors and never
rewrite text. Each entry owns its diagnostic title and message.

```yaml
forbidden_characters:
  - characters: ["\u2014"]
    title: Use natural phrasing without em dashes
    message: |
      Write like a person talking to another person, not an AI composing a response.
      Avoid em dashes. Use direct sentences and a natural, conversational rhythm.
      Rewrite rather than swapping punctuation mechanically. Preserve meaning
      and technical precision.
```

A configured list replaces the default; `[]` disables matches. Each entry
requires a nonempty list of single Unicode scalar values and nonblank `title`
and `message`. Duplicate characters are rejected, including within an entry.

Select `TEXT009` with the normal lint controls. Findings include the offending
character, its Unicode code point, and its line number. Custom messages
are preserved without appended generic advice.

### TEXT009 CLI output

With the default policy and a file containing `Read—this`:

```text
$ cargo run -p rust-llm-tidy-cli -- --checks-only --include TEXT009 example.md
example.md:1: error[TEXT009]: Use natural phrasing without em dashes: forbidden character '—' (U+2014).
Why: Direct sentences and a conversational rhythm make writing easier to follow.
Suggestions:
- Write like a person talking to another person, not an AI composing a response.
- Avoid em dashes. Use direct sentences and a natural, conversational rhythm.
- Rewrite the sentence, using a comma, colon, parentheses, or full stop where it fits the meaning.
- Do not mechanically replace every dash with the same punctuation. Preserve the meaning and technical precision. (file)
Error: found 1 error(s)
```

### Remarks

Headings and table prose are checked; code blocks, inline code, and link
destinations are skipped.

## Library access

Use `rust_llm_tidy::rules::lint::{run_text_checks, run_region_checks}`.
For complete processing and project context, see [library entry points].

[library entry points]: architecture.md#library-entry-points
