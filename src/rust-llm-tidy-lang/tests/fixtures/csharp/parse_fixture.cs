// License header kept as the preamble fixture: the parse must leave it in
// the preamble and pin the first item's own /// docs to the item.
// (Plain // comments are preamble material; /// comments attach.)
using System;
using System.IO;

namespace Fixtures;

/// <summary>Loads configuration values.</summary>
/// <param name="path">The file to read.</param>
/// <exception cref="System.IO.FileNotFoundException">Thrown when missing.</exception>
public class ConfigLoader
{
    private readonly int fallback;

    public ConfigLoader(int fallback) { this.fallback = fallback; }

    /// <summary>Reads a value or returns the fallback.</summary>
    /// <param name="key">The key to look up.</param>
    public int Read(string key) { return fallback; }
}
