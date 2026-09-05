// Rule: Usings inside a namespace body pin above the namespace's types.
//
// Members before reorder:
// - class Crate
// - using System.IO
// - struct Slot
//
// Members after reorder:
// - using System.IO
// - class Crate
// - struct Slot
//
// Notes:
// - A block-scoped namespace body reorders like a type body: usings
//   pin first while the types keep their source order.
//
namespace Inventory
{

    using System.IO;
    class Crate { }

    struct Slot { }
}
