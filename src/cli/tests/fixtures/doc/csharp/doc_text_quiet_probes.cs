// C# text checks stay quiet on trusted-producer guards: idiomatic XML
// docs, <code>/<example> blocks, long attribute values, and verbatim
// and interpolated string content are never measured as prose.
namespace Fixtures;

/// <summary>Loads values from the configured source.</summary>
public class Loader
{
    /// <summary>Loads a value for the key.</summary>
    /// <param name="key">The key to look up.</param>
    /// <exception cref="System.InvalidOperationException">
    /// Thrown when the key is empty.
    /// </exception>
    /// <returns>The loaded value.</returns>
    /// <remarks>
    /// The lookup walks the source once and retries transient failures.
    /// </remarks>
    public string Load(string key)
    {
        var note = "a // marker-shaped string with plenty of filler words that a marker scanner would happily measure as doc prose";
        return key + note;
    }

    /// <summary>Saves the value under the key in the store.</summary>
    /// <param name="key">The key.</param>
    /// <param name="value">The value.</param>
    /// <param name="store">The target store for the save operation.</param>
    /// <exception cref="System.Collections.Generic.KeyNotFoundException">
    /// Missing key.
    /// </exception>
    /// <exception cref="System.ArgumentNullException">
    /// Null key.
    /// </exception>
    /// <seealso cref="System.Collections.Generic.Dictionary{TKey,TValue}.Add"/>
    public void Save(string key, string value, string store) { }

    /// <summary>Runs the documented sample.</summary>
    /// <example>
    /// <code>
    /// var loader = new Loader();
    /// var value = loader.Load("key"); // trailing note in code
    /// if (value.Length < 3) { return; }
    /// </code>
    /// </example>
    public void Sample() { }

    /// <summary>Returns the message template.</summary>
    public string Template()
    {
        var id = $@"Interpolated header.
/// A doc-looking line inside the interpolated string carrying plenty of words
/// that a marker scanner would join and measure as one long paragraph of doc
/// prose, stretching far past the two hundred forty character budget so any
/// regression measuring string content must fail the quiet-probe fixture.
Tail.";
        return id + @"Verbatim first line.
/// A doc-looking line inside the verbatim string carrying plenty of words
/// that a marker scanner would join and measure as one long paragraph of
/// doc prose, stretching far past the two hundred forty character budget
/// so any regression measuring string content must fail this fixture.
Last line.";
    }
}

/// <summary>Kinds of source.</summary>
public enum SourceKind
{
    /// <summary>The primary source.</summary>
    Primary,

    /// <summary>The fallback source.</summary>
    Fallback,
}
