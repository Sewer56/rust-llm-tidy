# `reorder` for C# - profile-ordered types and members

C# files reorder through the same `reorder` op as Rust, with the C#
ordering profile.

At the top level, `using` directives pin first and everything else
keeps its file order; members inside a type order by the
Rider/ReSharper default buckets.

The pinned and protected rules below cover what never moves and what
declines outright.

## Top-level order

- `using` directives pin first, packed without blank lines, in their
  original relative order (they never reorder among themselves). A
  `using` that appears mid-file hoists up to the block.
- Hoisting a using only widens the names it introduces to earlier lines.
- Namespaces, type declarations, preprocessor directives, and top-level
  statements keep their source order. Types never dependency-sort, so a
  file's type layout is never reshuffled.

## Member order inside a top-level type

```csharp
public class Service
{
    // 1. Fields (const, static, and instance).
    private readonly int _id;

    // 2. Constructors.
    public Service(int id) { _id = id; }

    // 3. Finalizers.
    ~Service() { }

    // 4. Delegates and events.
    public delegate void Notify(object sender);
    public event Notify Changed;

    // 5. Enums and nested types.
    public enum Kind { Open, Closed }

    // 6. Properties and indexers.
    public int Count { get; set; }

    // 7. Operators.
    public static Service operator +(Service a, Service b) => a;

    // 8. Methods, callers before callees.
    public void Run() { Apply(); }
    void Apply() { }
}
```

Methods that reference a sibling method precede it; methods with no
references between them keep their file order. Namespace bodies apply
the same table, so nested `using` directives hoist above the namespace's
types.

A type nested inside another type moves as one member of the enclosing
body: its own members keep their order.

A type whose members do not each sit on their own lines (compact
one-line bodies) also keeps its member order, since line-tiled spans
cannot represent such a body.

## Pinned and protected

- A member's blank lines and `///` doc comments travel with it.
- Nothing moves across a preprocessor conditional (`#if`/`#else`/
  `#endif`): the parser groups each conditional run into one unit.
- A type or namespace body holding any preprocessor directive stays
  whole rather than permuting its members.
- A source the engine cannot vouch for degrades to a no-op: zero
  change records, no write. That includes files with parse errors and
  files whose preprocessor-region scan rejects them.
- Reordering is idempotent: a second run emits zero change records.

## Before

```csharp
using System;

namespace Demo.Services;

public class OrderService
{
    public void Run() { Apply(); }

    void Apply() { }

    public int Total { get; set; }

    private readonly int _id;
}
using System.IO;
```

## After

```csharp
using System;
using System.IO;

namespace Demo.Services;

public class OrderService
{

    private readonly int _id;

    public int Total { get; set; }
    public void Run() { Apply(); }

    void Apply() { }
}
```

The trailing `using System.IO;` hoists into the pinned block, the field
moves ahead of the property, and the caller `Run` stays before its
callee `Apply`.

Member slices are verbatim: each member keeps the blank lines that
preceded it in the source. So `_id` opens the body with its blank line,
and `Run` follows `Total` with none, since `Run` was the first member
in the source.
