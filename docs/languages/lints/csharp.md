# `lints` for C# - XML doc-comment checks

Doc comments are `///` lines, parameters are documented with
`<param name="...">` tags, and thrown exceptions with `<exception>`
tags.

A file with parse errors produces no findings rather than reporting
against misread declarations.

## Codes

| Code      | Severity | Fires when                                                                                       |
| --------- | -------- | ------------------------------------------------------------------------------------------------ |
| `DOC001`  | Error    | A non-private documentable member has no `///` comment.                                          |
| `DOC002`  | Error    | A non-private method or constructor that can throw has no `<exception>` tag.                     |
| `DOC003`  | Warning  | A non-private can-throw member's `<exception>` tags all lack a concrete `cref` type.             |
| `DOC004`  | Warning  | A non-private member with parameters has no `<param>` tags.                                      |
| `DOC005`  | Warning  | The `<param>` tags omit a declared parameter.                                                    |
| `DOC006`  | Warning  | A doc comment contains `TODO`, `FIXME`, or `TBD`.                                                |
| `TEXT001` | Error    | An XML doc text paragraph over 240 chars of inner text.                                          |
| `TEXT002` | Warning  | A doc line whose tag-stripped inner text exceeds 80 chars.                                       |
| `TEXT003` | Warning  | A doc sentence whose tag-stripped inner text exceeds 25 words.                                   |
| `TEXT006` | Hint     | Shorter-wording suggestions for words, phrases, redundancies, and filler in doc text             |
| `TEST001` | Warning  | A `TestMethod`/`Test`/`Fact`/`Theory` method uses a `test_*`, `case_*`, or `test` + digits name. |
| `MOD003`  | Hint     | A path includes the full namespace.                                                              |
| `SYM`     | Reminder | A configured text or symbol hint matches; severity is configurable.                              |

Errors fail the run with a non-zero exit; warnings, hints, and reminders do
not.

## Examples

Each example shows the smallest common fix for its lint. The text
lints' shared rules: [text lints].

Measurement details for the text lints:

- Lints measure `///` text-node inner text.
- `<code>` and `<example>` subtrees are never measured.

For Markdown README fences, see [TEXT005 guidance and examples]. TEXT005 does
not apply to C# XML doc regions.

[TEXT005 guidance and examples]: ../../text-lints.md#text005---fenced-code-block-without-a-language-tag

### DOC001 - missing documentation

A non-private documentable member has no `///` comment.

- Explicit `private` and modifier-less members pass.
- `internal` and `protected`-family count as non-private.

Documentable kinds: classes, structs, interfaces, records, enums,
delegates, methods, properties, events, fields, constructors.

Before:

```csharp
public class Loader
{
}
```

After:

```csharp
/// <summary>Loads configured values from storage.</summary>
public class Loader
{
}
```

#### DOC001 CLI output

```text
$ rust-llm-tidy --no-config --include DOC001 Loader.cs
Loader.cs:1: error[DOC001]: non-private item is missing a doc comment.

Why: Readers need its purpose and contract without tracing the implementation.

Suggestions:
- Add `/// <summary>` docs describing its purpose and supported contract. (class `Loader`)
Error: found 1 error(s)
```

### DOC002 - missing `<exception>` tag

Before:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    public int Load(string key)
    {
        throw new System.InvalidOperationException("empty");
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    /// <exception cref="System.InvalidOperationException">
    /// Thrown when the key is empty.
    /// </exception>
    public int Load(string key)
    {
        throw new System.InvalidOperationException("empty");
    }
}
```

#### DOC002 CLI output

```text
$ rust-llm-tidy --no-config --include DOC002 Loader.cs
Loader.cs:3: error[DOC002]: member that can throw is missing an `<exception>` doc tag.

Why: Readers need to understand possible failures and when they occur.

Suggestions:
- Trace its throws and calls.
- Document each exception that can escape with `<exception cref="Type">` and the specific condition that causes it.
- Do not invent exceptions or change behavior to satisfy this lint. (fn `Load`)
Error: found 1 error(s)
```

#### Remarks

Throw detection follows calls transitively within the nearest
`.csproj` and its literal `ProjectReference` targets.

- Matching is name-only: overloads and same-named types collide.
- External libraries and framework APIs are not tracked.
- A caught `throw` still counts.
- MSBuild variables, wildcards, injected references, external compile
  lists, missing targets, and nested repositories are ignored.
- Only repository `.gitignore` rules apply.

### DOC003 - vague `<exception>` tag

DOC003 shares DOC002's recursive throw detection.

Before:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    /// <exception>Thrown when the key is empty.</exception>
    public int Load(string key)
    {
        throw new System.InvalidOperationException("empty");
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    /// <exception cref="System.InvalidOperationException">Thrown when the key is empty.</exception>
    public int Load(string key)
    {
        throw new System.InvalidOperationException("empty");
    }
}
```

#### DOC003 CLI output

```text
$ rust-llm-tidy --no-config --include DOC003 Loader.cs
Loader.cs:3: warning[DOC003]: `<exception>` doc tags name no concrete exception type (`cref`).

Why: Concrete exception types help readers connect failure conditions to handling code.

Suggestions:
- Verify the member and its calls.
- Set each `cref` to the actual exception type that can escape.
- Describe the specific condition that causes each exception.
- Do not invent exceptions or change behavior to satisfy this lint. (fn `Load`)
```

### DOC004 - missing `<param>` tags

A non-private method, constructor, or indexer with parameters has no
`<param>` tags.

Before:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    /// <param name="key">The key to look up.</param>
    public int Load(string key)
    {
        return 1;
    }
}
```

#### DOC004 CLI output

```text
$ rust-llm-tidy --no-config --include DOC004 Loader.cs
Loader.cs:3: warning[DOC004]: member with parameters is missing `<param>` doc tags.

Why: Readers need parameter roles and constraints to supply appropriate inputs.

Suggestions:
- Add a `<param name="...">` tag for each declared parameter, describing its role and any constraints supported by the existing contract. (fn `Load`)
```

### DOC005 - undocumented parameter

Before:

```csharp
public class Loader
{
    /// <summary>Renders a value.</summary>
    /// <param name="key">The key to look up.</param>
    public string Render(string key, int width)
    {
        return key;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>Renders a value.</summary>
    /// <param name="key">The key to look up.</param>
    /// <param name="width">The output width.</param>
    public string Render(string key, int width)
    {
        return key;
    }
}
```

#### DOC005 CLI output

```text
$ rust-llm-tidy --no-config --include DOC005 Loader.cs
Loader.cs:3: warning[DOC005]: parameter(s) not documented in `<param>` tags: `width`.

Why: Omitted parameters leave readers guessing how to supply those inputs.

Suggestions:
- Add a `<param name="...">` tag for each listed parameter, describing its role and any constraints supported by the existing contract. (fn `Render`)
```

### DOC006 - placeholder text

Before:

```csharp
public class Loader
{
    /// <summary>TODO: document loading behavior.</summary>
    public int Load()
    {
        return 1;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>Loads the value from configured storage.</summary>
    public int Load()
    {
        return 1;
    }
}
```

#### DOC006 CLI output

```text
$ rust-llm-tidy --no-config --include DOC006 Loader.cs
Loader.cs:3: warning[DOC006]: doc comment contains placeholder text (TODO/FIXME/TBD).

Why: Placeholders leave readers without an explanation of current behavior.

Suggestions:
- Replace the placeholder with an accurate description of the existing contract. (fn `Load`)
```

### TEXT001 - oversized paragraph

An XML doc text paragraph over 240 chars of inner text. A paragraph is
one contiguous text run inside a tag: prose never joins across a tag
boundary.

Before:

```csharp
public class Loader
{
    /// <summary>
    /// Loads the configured value from persistent storage, validates it
    /// against the active schema, resolves relative resource paths against
    /// the config directory, retries transient failures with bounded
    /// backoff, and logs a short summary line once the load settles.
    /// </summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>Loads the configured value and validates it.</summary>
    /// <remarks>
    /// - Resolves relative resource paths against the config directory.
    /// - Retries transient failures with bounded backoff, then logs a
    ///   short summary line once the load settles.
    /// </remarks>
    public int Load(string key)
    {
        return 1;
    }
}
```

#### TEXT001 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT001 Loader.cs
Loader.cs:4: error[TEXT001]: paragraph is 256 chars long.
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

### TEXT002 - long line

Before:

```csharp
public class Loader
{
    /// <summary>Loads the value for the given key from the primary cache, falling back to the secondary cache when the primary is cold.</summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>
    /// Loads the value for the given key from the primary cache, falling
    /// back to the secondary cache when the primary is cold.
    /// </summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

#### TEXT002 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT002 Loader.cs
Loader.cs:3: warning[TEXT002]: line is 119 chars long.
Why: Long lines are harder to follow in narrow editors and side-by-side reviews.
Suggestions:
  - Wrap prose at word boundaries to 80 chars or fewer per line.
  - Preserve paragraph and list structure; do not split code identifiers, code spans, or URLs.
  - Code spans, URLs, and link targets count.
  - Code blocks, table rows, and link definitions are exempt.
  - Borders are ignored. (file)
```

### TEXT003 - long sentence

A sentence over 25 words of tag-stripped inner text is a warning;
wrapped `///` lines join into one sentence.

Before:

```csharp
public class Loader
{
    /// <summary>
    /// Loads the configured value from persistent storage, validates every
    /// parsed field, resolves relative resource paths, retries transient
    /// failures with bounded backoff, and logs one summary line.
    /// </summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>
    /// Loads the configured value from persistent storage and validates
    /// every parsed field.
    /// </summary>
    /// <remarks>
    /// - Resolves relative resource paths.
    /// - Retries transient failures with bounded backoff, then logs one
    ///   summary line.
    /// </remarks>
    public int Load(string key)
    {
        return 1;
    }
}
```

#### TEXT003 CLI output

```text
$ rust-llm-tidy --no-config --include TEXT003 Loader.cs
Loader.cs:4: warning[TEXT003]: sentence is 26 words long.
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

### TEXT006 - verbose synonyms

A measured `///` line can receive a shorter-wording hint for its inner text.

The [TEXT006 rule] defines the shared dictionary and matching behavior.
Hints show before/after guidance without changing the file.

Before:

```csharp
public class Loader
{
    /// <summary>We utilize this loader in order to show values.</summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

After:

```csharp
public class Loader
{
    /// <summary>We use this loader to show values.</summary>
    public int Load(string key)
    {
        return 1;
    }
}
```

#### TEXT006 CLI output

```text
$ cargo run -p rust-llm-tidy-cli -- --include TEXT006 Loader.cs
Loader.cs:3: hint[TEXT006]: wording has a simpler alternative: `utilize`.
Why: Unnecessary formal wording and framing can make the point harder to understand.
Suggestions:
  - Before: `utilize`
  - After: `use`
  - Preserve meaning and adjust grammar to fit.
  - Use the alternative only if it preserves technical meaning, uncertainty, and required wording. (file)
```

[TEXT006 rule]: ../../text-lints.md#text006---verbose-synonyms

### TEST001 - non-behavioral test name

A `TestMethod`/`Test`/`Fact`/`Theory` method uses a `test_*`, `case_*`,
or `test` + digits name.

- Marker attributes match with the `Attribute` suffix stripped.
- Names evaluate case-insensitively.

Before:

```csharp
public class LoaderTests
{
    [TestMethod]
    public void test_load()
    {
    }
}
```

After:

```csharp
public class LoaderTests
{
    [TestMethod]
    public void load_returns_the_value_for_a_known_key()
    {
    }
}
```

#### TEST001 CLI output

```text
$ rust-llm-tidy --no-config --include TEST001 Loader.cs
Loader.cs:3: warning[TEST001]: test method `test_load` uses a discouraged naming pattern.

Why: Behavioral names help readers understand a test's claim without opening its body.

Suggestions:
- Rename it to describe the subject and expected behavior, adding a condition only when it matters.
- Use `subject_should_expectation[_when_condition]` in the project's casing style. (fn `test_load`)
```

### Multiple findings at once

Before:

```csharp
public class Loader
{
    /// <summary>Loads a value.</summary>
    public int Load(string key)
    {
        throw new System.InvalidOperationException("empty");
    }
}
```

Findings:

```text
Loader.cs:1: error[DOC001]: non-private item is missing a doc comment.

Why: Readers need its purpose and contract without tracing the implementation.

Suggestions:
- Add `/// <summary>` docs describing its purpose and supported contract. (class `Loader`)
Loader.cs:3: error[DOC002]: member that can throw is missing an `<exception>` doc tag.

Why: Readers need to understand possible failures and when they occur.

Suggestions:
- Trace its throws and calls.
- Document each exception that can escape with `<exception cref="Type">` and the specific condition that causes it.
- Do not invent exceptions or change behavior to satisfy this lint. (fn `Load`)
Loader.cs:3: warning[DOC004]: member with parameters is missing `<param>` doc tags.

Why: Readers need parameter roles and constraints to supply appropriate inputs.

Suggestions:
- Add a `<param name="...">` tag for each declared parameter, describing its role and any constraints supported by the existing contract. (fn `Load`)
```

After:

```csharp
/// <summary>Loads configuration values.</summary>
public class Loader
{
    /// <summary>Loads a value.</summary>
    /// <param name="key">The key to look up.</param>
    /// <exception cref="System.InvalidOperationException">
    /// Thrown when the key is empty.
    /// </exception>
    public int Load(string key)
    {
        throw new System.InvalidOperationException("empty");
    }
}
```

#### Remarks

The `Load` findings anchor at line 3, its `///` doc line. An item starts at
its doc comment, so its start line is the doc run's first line.

[text lints]: ../../text-lints.md

### MOD003 - full namespace qualification in code

Flags paths that include the full namespace;
partial paths and aliases are allowed at any depth.

Recognized roots:

- `global::`, including custom namespaces
- `System` and `Microsoft`, unless shadowed or potentially relative

Other unprefixed roots stay silent. Imports, type names, and namespace
declarations do not prove a full root.

Before (`example.cs`):

```csharp
class C { System.Text.StringBuilder Create() => new(); }
```

After:

```csharp
using System.Text;

class C { StringBuilder Create() => new(); }
```

With a `config.yml` containing `{}`, the local CLI renders:

```text
$ cargo run -p rust-llm-tidy-cli -- --config config.yml --include MOD003 example.cs
example.cs:1: hint[MOD003]: path `System.Text.StringBuilder` includes the full namespace.
Why: full namespace prefixes give readers longer lines to scan before reaching the item name, making code harder to understand.
Suggestions:
- If `System.Text` is a namespace and the result is clear, add `using System.Text;` at namespace or file scope and use `StringBuilder`.
- Retain namespace or type context when needed; use a type alias if the proposed import targets a containing type, not a namespace.
- Keep the full path if shortening would reduce clarity or create a name conflict.
- Verify the shorter path resolves to the same symbol; this hint uses syntax, not compiler name resolution. (fn `Create`)
```

#### Remarks

Import advice:

- Missing imports: suggest a namespace `using`, conditional on the prefix
  being a namespace rather than a containing type.
- Namespace imports: `using System.Threading.Tasks;` shortens
  `System.Threading.Tasks.Task.Delay` to `Task.Delay`.
- Root imports: `using System;` shortens `System.Console.WriteLine` to
  `Console.WriteLine`.
- Aliases: `using Log = System.Console;` shortens
  `System.Console.WriteLine` to `Log.WriteLine`.
- Absolute paths: `global::Vendor.Net.Client` suggests
  `using global::Vendor.Net;` when the prefix is a namespace.

Type positions suggest importing the parent; expressions import only the root
to retain possible type and property segments. For containing types, use a type
alias or retain qualification instead.

Absolute paths only reuse explicitly absolute imports to avoid relative targets.

MOD003 skips:

- Imports and attributes.
- Paths whose shorter name would be ambiguous or shadowed.
- Expression receivers not recognized as namespace or type paths.
- Generic chains, rather than suggesting replacements that lose type arguments.
- Unprefixed roots also declared below another namespace anywhere in the file.
- Code guarded by `#if`, including the whole method containing the directive.

Hints never fail the run or rewrite source.
See [shared MOD003 policy] for Rust behavior.

[shared MOD003 policy]: ../../lints.md#mod003---full-namespace-qualification-in-code

## SYM - symbol rules

Add custom hints or exclude declarations with [symbol rules].

- [Array matching] covers C# array creations.
- [Built-in symbols] covers capacity and array reminders.

[symbol rules]: ../../symbol-rules.md
[Array matching]: ../../symbol-rules.md#usage-constraints
[Built-in symbols]: ../../symbol-rules.md#built-in-symbols

## Library access

Use `rust_llm_tidy::tidy_source` with the `lints` operation and `cs` extension.
For complete processing and project context, see [library entry points].

[library entry points]: ../../architecture.md#library-entry-points
