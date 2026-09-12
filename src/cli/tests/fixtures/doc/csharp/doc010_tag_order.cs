// DOC010 for C#: doc tags must follow the canonical order; the clean
// member passes, the member with <exception> before <param> fails.
namespace Fixtures;

/// <summary>Loads values.</summary>
public class Store
{
    /// <summary>Loads the value for a key.</summary>
    /// <param name="key">The key to load.</param>
    /// <returns>The stored value.</returns>
    /// <exception cref="System.Exception">When the key is missing.</exception>
    public string Load(string key) { return ""; }

    /// <summary>Saves a value.</summary>
    /// <exception cref="System.Exception">When the store is full.</exception>
    /// <param name="key">The key to save under.</param>
    public void Save(string key) { }
}
