// DOC002 for C#: a documented non-private member whose body throws needs
// an <exception> tag; private throwers and tagged throwers pass.
namespace Fixtures;

/// <summary>Loads values.</summary>
public class Loader
{
    /// <summary>Loads without documenting the throw.</summary>
    public void Untagged()
    {
        throw new System.InvalidOperationException("empty");
    }

    /// <summary>Loads and documents the throw.</summary>
    /// <exception cref="System.InvalidOperationException">Thrown when empty.</exception>
    public void Tagged()
    {
        throw new System.InvalidOperationException("empty");
    }

    /// <summary>Private throwers are skipped.</summary>
    private void Hidden()
    {
        throw new System.InvalidOperationException("empty");
    }
}
