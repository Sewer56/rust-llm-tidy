// Rule: A body written compact stays compact after the reorder.
//
// Members before reorder:
// - method Run (calls Build)
// - method Build
// - field _runs
//
// Members after reorder:
// - field _runs
// - method Run
// - method Build
//
// Notes:
// - Member slices are verbatim: no blank lines are inserted between
//   members that had none.
//
class Compact
{
    void Run() { Build(); }
    void Build() { }
    int _runs;
}
