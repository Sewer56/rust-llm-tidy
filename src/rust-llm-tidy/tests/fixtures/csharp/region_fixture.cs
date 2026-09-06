// Preprocessor interaction: every conditional boundary must freeze the
// enclosing body, and nothing may move across a directive line.
using System;

#if DEBUG
class DebugOnly { }
#endif

public class Mixed
{
    private int first;

#if TRACE
    public int Traced { get; set; }
#endif

    public void After() { }
}
