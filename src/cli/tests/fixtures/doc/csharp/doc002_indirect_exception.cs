// DOC002 recursion for C#: callers of same-file throwing members need
// an <exception> tag even when their own body holds no throw; private
// throwers, framework calls, and tagged callers pass.
namespace Fixtures;

/// <summary>Loads values.</summary>
public class Loader
{
    /// <summary>Throws on an empty input.</summary>
    /// <exception cref="System.InvalidOperationException">Empty input.</exception>
    private void Validate()
    {
        throw new System.InvalidOperationException("empty");
    }

    /// <summary>Calls the private thrower without documenting the throw.</summary>
    public void Load()
    {
        Validate();
    }

    /// <summary>Reaches the throw one call further away.</summary>
    public void LoadTwice()
    {
        Load();
        Load();
    }

    /// <summary>Calls only framework code.</summary>
    public void Parse()
    {
        int.Parse("");
    }

    /// <summary>Documents the throw reached through Validate.</summary>
    /// <exception cref="System.InvalidOperationException">Empty input.</exception>
    public void LoadGuarded()
    {
        Validate();
    }
}
