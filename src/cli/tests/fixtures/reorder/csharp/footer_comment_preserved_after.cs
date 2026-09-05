// Rule: The file footer stays below the reordered items.
//
// Items before reorder:
// - class Keeper
// - using System
//
// Items after reorder:
// - using System
// - class Keeper
//
// Notes:
// - Everything after the last item is trailer: the hoisting using
//   leaves the footer at the end of the file.
//
using System;

class Keeper
{
    void Keep() { }
}
// Footer note: stays after the last item.
