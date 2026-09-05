// C# reorder: the type's members arrive out of profile order and one
// method calls another. Reordering puts fields, constructors, finalizers,
// delegates/events, enums, properties, operators, then methods with the
// caller first; the trailing using hoists up to the pinned using block.
using System;
using System.IO;

namespace Demo.Services;

/// <summary>Applies ordering rules to orders.</summary>
public class OrderService
{

    private readonly int _id;

    public OrderService(int total) { Total = total; }

    ~OrderService() { }

    public delegate void Notify(object sender);

    public event EventHandler Changed;

    public enum Status { Open, Closed }

    public int Total { get; set; }

    public static OrderService operator +(OrderService a, OrderService b) => a;
    public void Run() { Apply(); }

    private void Validate() { }

    void Apply() { }
}
