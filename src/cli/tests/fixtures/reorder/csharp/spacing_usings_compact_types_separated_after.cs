// Rule: Consecutive usings pack; every other top-level item separates.
//
// Items before reorder:
// - using System
// - using System.IO
// - class Packed
// - class Cramped
//
// Items after reorder:
// - using System
// - using System.IO
// - class Packed
// - class Cramped
//
// Notes:
// - The usings were already adjacent and stay packed.
// - A blank line is inserted between the crammed-together classes.
//
using System;
using System.IO;

class Packed { }

class Cramped { }
