// Rule: Enums and nested types keep their source order in one bucket.
//
// Members before reorder:
// - enum Zebra
// - class Alpha
// - struct Moon
// - interface IKind
//
// Members after reorder:
// - unchanged
//
// Notes:
// - The enum/nested-type bucket is stable: no alphabetical sort.
//
class Registry
{
    public enum Zebra { One }

    public class Alpha { }

    public struct Moon { }

    public interface IKind { }
}
