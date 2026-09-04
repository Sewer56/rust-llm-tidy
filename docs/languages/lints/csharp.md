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
| `DOC002`  | Error    | A non-private method or constructor whose body throws has no `<exception>` tag.                  |
| `DOC003`  | Warning  | A non-private throwing member's `<exception>` tags all lack a concrete `cref` type.              |
| `DOC004`  | Warning  | A non-private member with parameters has no `<param>` tags.                                      |
| `DOC005`  | Warning  | The `<param>` tags omit a declared parameter.                                                    |
| `DOC006`  | Warning  | A doc comment contains `TODO`, `FIXME`, or `TBD`.                                                |
| `TEXT001` | Error    | An XML doc text paragraph over 240 chars of inner text.                                          |
| `TEXT002` | Warning  | A doc line whose tag-stripped inner text exceeds 80 chars.                                       |
| `TEST001` | Warning  | A `TestMethod`/`Test`/`Fact`/`Theory` method uses a `test_*`, `case_*`, or `test` + digits name. |

Error-severity codes fail the run with a non-zero exit; warnings exit 0.

## Examples

Each example shows the smallest common fix for its lint. The text
lints measure `///` text-node inner text, and `<code>` and `<example>`
subtrees are never measured.

The text lints' shared rules: [text lints].

### DOC001 - missing documentation

A non-private documentable member has no `///` comment. Explicit
`private` and modifier-less members pass; `internal` and
`protected`-family count as non-private.

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
Loader.cs:1: error[DOC001]: non-private item is missing a doc comment (class `Loader`)
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
Loader.cs:3: error[DOC002]: member that throws is missing an `<exception>` doc tag (fn `Load`)
Error: found 1 error(s)
```

The body scan is a heuristic: rethrows in called helpers can be
missed, and throws caught in a local `try`/`catch` still fire.

### DOC003 - vague `<exception>` tag

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
Loader.cs:3: warning[DOC003]: `<exception>` doc tags name no concrete exception type (`cref`) (fn `Load`)
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
Loader.cs:3: warning[DOC004]: member with parameters is missing `<param>` doc tags (fn `Load`)
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
Loader.cs:3: warning[DOC005]: parameter(s) not documented in `<param>` tags: `width` (fn `Render`)
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
Loader.cs:3: warning[DOC006]: doc comment contains placeholder text (TODO/FIXME/TBD) (fn `Load`)
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
  - Paragraphs over 240 chars outlast a short attention span.
  - Split it at the nearest idea change with a blank line.
  - Convert list-like paragraphs into bullets.
  - Keep each bullet to one checkable action of at most 160 chars.
  - Move remarks into their own sections.
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
  - Lines over 80 chars strain short attention spans and need wide monitors.
  - Split it at the nearest idea change with a blank line.
  - Code spans, URLs, and link targets count.
  - Code blocks, table rows, and link definitions are exempt. (file)
```

### TEST001 - non-behavioral test name

A `TestMethod`/`Test`/`Fact`/`Theory` method uses a `test_*`, `case_*`,
or `test` + digits name. Marker attributes match with the `Attribute`
suffix stripped, and names evaluate case-insensitively.

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
Loader.cs:3: warning[TEST001]: test method `test_load` should use a behavioral name (subject_should_expectation_when_condition), not a `test_*` or `case_*` prefix (fn `test_load`)
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
Loader.cs:1: error[DOC001]: non-private item is missing a doc comment (class `Loader`)
Loader.cs:3: error[DOC002]: member that throws is missing an `<exception>` doc tag (fn `Load`)
Loader.cs:3: warning[DOC004]: member with parameters is missing `<param>` doc tags (fn `Load`)
```

The `Load` findings anchor at line 3, its `///` doc line: an item starts at
its doc comment, so its start line is the doc run's first line.

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

[text lints]: ../../text-lints.md
