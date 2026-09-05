// Rule: Pinned `using` directives never sort among themselves.
//
// Items before reorder:
// - using System.Xml
// - class Catalog
// - using System.Collections
//
// Items after reorder:
// - using System.Xml
// - using System.Collections
// - class Catalog
//
// Notes:
// - Hoisting keeps source order: System.Xml stays ahead of the
//   alphabetically-earlier System.Collections.
//
using System.Xml;
using System.Collections;

class Catalog { }
