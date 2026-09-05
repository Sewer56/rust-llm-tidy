// DOC003 for C#: <exception> tags without a concrete cref are vague; any
// non-empty cref passes.
namespace Fixtures;

/// <summary>Parses text.</summary>
public class Parser
{
    /// <summary>Parses with a vague tag.</summary>
    /// <exception>Thrown when invalid.</exception>
    public void Vague()
    {
        throw new System.FormatException("bad");
    }

    /// <summary>Parses with a concrete cref.</summary>
    /// <exception cref="System.FormatException">Thrown when invalid.</exception>
    public void Concrete()
    {
        throw new System.FormatException("bad");
    }
}
