// Rule: Fields keep their source order within the field bucket.
//
// Members before reorder:
// - field Zeal
// - field Alpha
// - field _moon
//
// Members after reorder:
// - unchanged
//
// Notes:
// - No alphabetical sort: Zeal stays ahead of Alpha.
// - const, static readonly, and instance fields share one bucket.
//
class Fields
{
    const int Zeal = 1;

    static readonly string Alpha = "a";

    private int _moon;
}
