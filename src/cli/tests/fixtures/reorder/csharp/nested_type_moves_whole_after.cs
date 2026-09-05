// Rule: A nested type moves whole; its own members never resort.
//
// Members before reorder:
// - method Finish
// - class Inner (body: Second, First, Tally)
// - field _seed
//
// Members after reorder:
// - field _seed
// - class Inner, body unchanged
// - method Finish
//
// Notes:
// - The outer body reorders by bucket, moving Inner as one member.
// - Inner's callee stays before its caller and its property stays
//   after its methods: no nested-body sort runs.
//
class Host
{

    private int _seed;

    class Inner
    {
        void Second() { }

        void First() { Second(); }

        int Tally { get; set; }
    }
    void Finish() { }
}
