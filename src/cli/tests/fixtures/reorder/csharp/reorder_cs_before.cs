// C# reorder: the type's members arrive out of profile order and one
// method calls another. Reordering puts fields, constructors, finalizers,
// delegates/events, enums, properties, operators, then methods with the
// caller first; the trailing using hoists up to the pinned using block.
using System;

namespace Demo.Services;

/// <summary>Applies ordering rules to orders.</summary>
public class OrderService
{
    public void Run() { Apply(); }

    void Apply() { }

    public static OrderService operator +(OrderService a, OrderService b) => a;

    public int Total { get; set; }

    public enum Status { Open, Closed }

    public delegate void Notify(object sender);

    public event EventHandler Changed;

    ~OrderService() { }

    public OrderService(int total) { Total = total; }

    private readonly int _id;

    private void Validate() { }
}
using System.IO;
