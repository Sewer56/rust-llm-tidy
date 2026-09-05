// Rule: Methods sort callers ahead of callees.
//
// Members before reorder:
// - method Plan
// - method Build (calls Plan)
// - method Run (calls Build)
//
// Members after reorder:
// - method Run
// - method Build
// - method Plan
//
// Notes:
// - The call chain reverses so each caller is read before the method
//   it invokes.
//
class Pipeline
{
    void Plan() { }

    void Build() { Plan(); }

    void Run() { Build(); }
}
