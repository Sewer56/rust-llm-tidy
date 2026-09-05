// Rule: Top-level `using` directives pin ahead of every other item.
//
// Items before reorder:
// - using System.Text
// - class Alpha
// - using System
// - interface IBeta
// - using System.IO
//
// Items after reorder:
// - using System.Text
// - using System
// - using System.IO
// - class Alpha
// - interface IBeta
//
// Notes:
// - The mid-file and trailing usings hoist above the types; the using
//   block keeps its source order.
//
using System.Text;

class Alpha { }

using System;

interface IBeta { }

using System.IO;
