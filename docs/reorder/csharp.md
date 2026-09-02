# `reorder` for C# - profile-ordered types and members

C# files reorder through the same `reorder` op as Rust; shared behavior
(config, change output, callers before callees) is in [reorder]. Members
follow the Rider/ReSharper default type member order; method ties keep
file order, not Rust's alphabetical.

## Top-level order

- Namespaces keep source order: `namespace Demo.Services;`
- Type declarations keep source order: `class Service { }`, `record Point(...)`
- Preprocessor directives keep source order: `#if DEBUG ... #endif`
- Top-level statements keep source order: `await Run();`

## Member order inside a type

```csharp
using System;
using System.IO;

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

- Namespace bodies apply the same table.
- A nested type moves as one member; its own members keep their order.
- A type whose members do not each sit on their own lines (compact
  one-line bodies) keeps its member order.

## Pinned and protected

- A member's blank lines and `///` doc comments travel with it.
- Nothing moves across a preprocessor conditional (`#if`/`#else`/
  `#endif`); a body holding any preprocessor directive stays whole.
- Files with parse errors, or that the preprocessor-region scan rejects,
  degrade to a no-op: zero change records, no write.
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

Each member keeps the blank lines that preceded it in the source.

[reorder]: ../reorder.md
