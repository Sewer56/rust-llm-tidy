// Rule: Type members land in the profile's bucket order.
//
// Members before reorder:
// - method Run
// - operator +
// - property Size
// - enum State
// - event Ticked
// - delegate Tick
// - finalizer
// - constructor
// - field _count
//
// Members after reorder:
// - field _count
// - constructor
// - finalizer
// - event Ticked
// - delegate Tick
// - enum State
// - property Size
// - operator +
// - method Run
//
// Notes:
// - Buckets order fields, constructors, finalizers, delegates/events,
//   enums/nested types, properties, operators, then methods.
// - Ticked stays ahead of Tick: one bucket keeps source order.
//
class Widget
{
    public void Run() { }

    public static Widget operator +(Widget a, Widget b) => a;

    public int Size { get; set; }

    public enum State { On, Off }

    public event EventHandler Ticked;

    public delegate void Tick(object sender);

    ~Widget() { }

    public Widget() { }

    private int _count;
}
