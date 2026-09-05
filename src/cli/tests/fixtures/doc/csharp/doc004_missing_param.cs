// DOC004 for C#: a documented non-private member with parameters needs
// <param> tags; members without parameters pass.
namespace Fixtures;

/// <summary>Greets people.</summary>
public class Greeter
{
    /// <summary>Greets without parameter tags.</summary>
    public void Greet(string name) { }

    /// <summary>Greets with parameter tags.</summary>
    /// <param name="name">The name to greet.</param>
    public void Greeted(string name) { }

    /// <summary>No parameters, no tags needed.</summary>
    public void NoArgs() { }
}
