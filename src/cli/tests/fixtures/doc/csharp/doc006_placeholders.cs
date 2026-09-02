// DOC006 for C#: placeholder markers in doc comments read as finished API
// documentation.
namespace Fixtures;

/// <summary>Holds task methods.</summary>
public class Tasks
{
    /// <summary>TODO: describe this.</summary>
    public void Todo() { }

    /// <summary>FIXME: describe this.</summary>
    public void Fixme() { }

    /// <summary>TBD: describe this.</summary>
    public void Tbd() { }

    /// <summary>Done: described.</summary>
    public void Done() { }
}
