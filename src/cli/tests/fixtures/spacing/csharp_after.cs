/// <summary>An edge in a flow graph.</summary>
public class Edge
{
    /// <summary>The source address.</summary>
    public int Source;

    /// <summary>The target address.</summary>
    public int Target;
}

/// <summary>How control leaves an instruction.</summary>
public enum Exit
{
    /// <summary>Follows the branch target.</summary>
    Taken,

    /// <summary>Falls through to the next instruction.</summary>
    NotTaken,
}
