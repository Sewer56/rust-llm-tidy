// Rule: The file header stays above the reordered items.
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
// - Everything above the first item is preamble: the hoisting using
//   lands below the header, never above it.
//
// File header: stays above the reorder.
// Second header line.

class Keeper
{
    void Keep() { }
}

using System;
