// DOC011 for C#: a documented non-private method with a non-void return
// value needs a <returns> tag; bool returns only remind; tagged, void,
// and private members pass.
namespace Fixtures;

/// <summary>Counts things.</summary>
public class Counter
{
    /// <summary>Returns the current count.</summary>
    public int GetCount() { return 0; }

    /// <summary>Reports whether the counter holds a value.</summary>
    public bool IsReady() { return false; }

    /// <summary>Returns the current count.</summary>
    /// <returns>The current count, always zero.</returns>
    public int Tagged() { return 0; }

    /// <summary>Resets the count to zero.</summary>
    public void Reset() { }

    private int Hidden() { return 0; }
}
