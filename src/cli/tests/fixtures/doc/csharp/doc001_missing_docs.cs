// DOC001 for C#: every non-private documentable member without a ///
// comment is flagged; private members, members without a modifier, and
// documented members pass.
namespace Fixtures;

/// <summary>Documented container.</summary>
public class Alpha
{
    public void Undocumented() { }

    private void Hidden() { }

    void InternalDefault() { }

    /// <summary>Documented.</summary>
    public void Documented() { }

    protected int Guarded { get; set; }

    internal static int Cached = 1;

    /// <summary>Documented container.</summary>
    public interface IBehavior
    {
        void Apply();
    }

    public struct Shape { }

    public enum Kind { A }

    public delegate void Notify(object sender);

    public event Notify Changed { add { } remove { } }

    public Alpha() { }
}
