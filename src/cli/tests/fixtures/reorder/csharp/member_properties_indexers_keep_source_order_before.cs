// Rule: Properties and indexers keep their source order in one bucket.
//
// Members before reorder:
// - property Zebra
// - property Alpha
// - indexer this[int moon]
//
// Members after reorder:
// - unchanged
//
// Notes:
// - The property/indexer bucket is stable: no alphabetical sort.
//
class Shelf
{
    public int Zebra { get; set; }

    public string Alpha => "a";

    public byte this[int moon] => 0;
}
