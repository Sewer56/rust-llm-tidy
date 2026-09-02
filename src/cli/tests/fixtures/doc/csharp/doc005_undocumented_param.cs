// DOC005 for C#: <param> tags must name every declared parameter.
namespace Fixtures;

/// <summary>Builds things.</summary>
public class Builder
{
    /// <summary>Builds with one parameter undocumented.</summary>
    /// <param name="name">The name.</param>
    public void Build(string name, string format) { }

    /// <summary>Builds with every parameter documented.</summary>
    /// <param name="name">The name.</param>
    /// <param name="format">The format.</param>
    public void Built(string name, string format) { }
}
