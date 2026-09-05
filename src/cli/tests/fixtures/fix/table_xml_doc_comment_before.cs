using System;

/// Options accepted by the demo loader.
///
/// | Name | Value | Description |
/// | --- | --- | --- |
/// | a | 1 | first |
/// | longname | 200 | second item |
public sealed class Options
{
    /// The parsed value.
    public int Value { get; set; }
}
