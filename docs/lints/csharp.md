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
| `DOC002`  | Warning  | A non-private method or constructor whose body throws has no `<exception>` tag.                  |
| `DOC003`  | Warning  | A non-private throwing member's `<exception>` tags all lack a concrete `cref` type.              |
| `DOC004`  | Warning  | A non-private member with parameters has no `<param>` tags.                                      |
| `DOC005`  | Warning  | The `<param>` tags omit a declared parameter.                                                    |
| `DOC006`  | Warning  | A doc comment contains `TODO`, `FIXME`, or `TBD`.                                                |
| `TEST001` | Warning  | A `TestMethod`/`Test`/`Fact`/`Theory` method uses a `test_*`, `case_*`, or `test` + digits name. |

## Semantics

- DOC001 counts explicit `public`, `internal`, and `protected`-family
  modifiers (including `protected internal` and `private protected`) as
  non-private. Explicit `private` and members without a modifier pass.
- Documentable kinds: classes, structs, interfaces, records, enums,
  delegates, methods, properties, events, fields, and constructors.
- DOC002 scans the member body for `throw` statements at warning
  severity: the body scan can miss rethrows through helpers, so an error
  would overfire. Documenting any `<exception>` tag satisfies it.
- DOC004 and DOC005 check the real parameter list of methods,
  constructors, and indexers against the `<param name="...">` tags.
- TEST001 matches the marker attributes with the customary `Attribute`
  suffix stripped (`[TestMethod]` and `[TestMethodAttribute]` both
  count), and evaluates the naming rule case-insensitively for C#'s
  PascalCase conventions.
- A file with parse errors produces no findings: the whole pass degrades
  to silence instead of reporting against misread declarations.
- The parser-free text checks (`DOC007`, `DOC008`) never run on `.cs`
  files.
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
Loader.cs:3: warning[DOC002]: member that throws is missing an `<exception>` doc tag (fn `Load`)
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
