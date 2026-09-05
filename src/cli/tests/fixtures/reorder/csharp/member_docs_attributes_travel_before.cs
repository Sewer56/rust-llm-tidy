// Rule: XML docs and attributes travel with their member.
//
// Members before reorder:
// - method Tally
// - method Add (calls Tally), with <summary> docs and [Obsolete]
//
// Members after reorder:
// - method Add, still carrying its docs and attribute
// - method Tally
//
// Notes:
// - The caller hoists above its callee with the doc lines and the
//   attribute list intact.
//
class Ledger
{
    int Tally() { return 0; }

    /// <summary>Adds an entry to the ledger.</summary>
    [Obsolete]
    public void Add(int amount) { Tally(); }
}
