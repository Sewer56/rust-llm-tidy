// Rule: Top-level types keep their source order; only usings move.
//
// Items before reorder:
// - interface IGauge
// - enum Scale
// - class Meter
// - delegate Tick
//
// Items after reorder:
// - unchanged
//
// Notes:
// - No top-level phase dependency-sorts or alphabetizes types, so this
//   file is already tidy.
//
interface IGauge { }

enum Scale { Celsius, Kelvin }

class Meter { }

delegate void Tick(object sender);
