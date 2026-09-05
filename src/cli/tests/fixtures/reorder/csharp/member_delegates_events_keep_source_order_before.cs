// Rule: Delegates and events keep their source order in one bucket.
//
// Members before reorder:
// - delegate Zebra
// - event Alpha
// - delegate Moon
//
// Members after reorder:
// - unchanged
//
// Notes:
// - The delegate/event bucket is stable: no alphabetical sort.
//
class Signals
{
    public delegate void Zebra(object sender);

    public event EventHandler Alpha;

    public delegate void Moon(object sender, int n);
}
