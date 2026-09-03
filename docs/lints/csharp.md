# `lints` for C# - XML doc-comment checks

C# files lint through the same `lints` op as Rust, with the same codes
and output shape, evaluated against the XML doc-comment dialect.

Doc comments are `///` lines, parameters are documented with
`<param name="...">` tags, and thrown exceptions with `<exception>`
tags.

## Codes

| Code      | Severity | Fires when                                                                                       |
| --------- | -------- | ------------------------------------------------------------------------------------------------ |
| `DOC001`  | Error    | A non-private documentable member has no `///` comment.                                          |
| `DOC002`  | Error    | A non-private method or constructor whose body throws has no `<exception>` tag.                  |
| `DOC003`  | Warning  | A non-private throwing member's `<exception>` tags all lack a concrete `cref` type.              |
| `DOC004`  | Warning  | A non-private member with parameters has no `<param>` tags.                                      |
| `DOC005`  | Warning  | The `<param>` tags omit a declared parameter.                                                    |
| `DOC006`  | Warning  | A doc comment contains `TODO`, `FIXME`, or `TBD`.                                                |
| `DOC007`  | Error    | An XML doc text paragraph over 240 chars of inner text.                                          |
| `DOC008`  | Warning  | A doc line whose tag-stripped inner text exceeds 80 chars.                                       |
| `TEST001` | Warning  | A `TestMethod`/`Test`/`Fact`/`Theory` method uses a `test_*`, `case_*`, or `test` + digits name. |

## Semantics

- DOC001 counts explicit `public`, `internal`, and `protected`-family
  modifiers (including `protected internal` and `private protected`) as
  non-private. Explicit `private` and members without a modifier pass.
- Documentable kinds: classes, structs, interfaces, records, enums,
  delegates, methods, properties, events, fields, and constructors.
- DOC002 scans the member body for `throw` statements at error
  severity. The scan is a heuristic in both directions: it misses
  rethrows performed by called helpers, and it fires on throws caught
  in a local `try`/`catch`. Documenting any `<exception>` tag
  satisfies it.
- DOC004 and DOC005 check the real parameter list of methods,
  constructors, and indexers against the `<param name="...">` tags.
- TEST001 matches the marker attributes with the customary `Attribute`
  suffix stripped (`[TestMethod]` and `[TestMethodAttribute]` both
  count), and evaluates the naming rule case-insensitively for C#'s
  PascalCase conventions.
- A file with parse errors produces no findings: the whole pass degrades
  to silence instead of reporting against misread declarations.
- The text checks (`DOC007`, `DOC008`) measure `///` doc-comment prose
  with the XML doc dialect, over the same parse as the other codes:
  - only the inner text of text nodes counts: tags are stripped and
    attribute values (`cref`, `name`, ...) are excluded;
  - `<code>` and `<example>` subtrees are exempt like code fences;
  - a paragraph is a contiguous text run within one tag: prose never
    joins across a tag boundary, and blank `///` lines split paragraphs;
  - string literals and code lines are never comment nodes, so their
    content is never measured;
  - findings carry original file lines.
- JSON records reuse the Rust rule titles (DOC002 shows
  `missing \`# Errors\` section`, DOC004
  `missing \`# Arguments\` section`): codes, record shape, and titles are
  shared across languages, and the message text carries the C# fix.

## Example

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
