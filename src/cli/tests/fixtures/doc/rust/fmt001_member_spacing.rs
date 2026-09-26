//! FMT001 fixture: documented members without blank lines between them.
//!
//! The struct, enum, and impl body each carry doc comments on
//! consecutive members with no blank line separating them, so each
//! flags exactly one warning.

/// An edge in the flow graph.
pub struct LocalEdge {
    /// Address of the instruction taking this path.
    pub source: u32,
    /// Address of the first instruction at the destination.
    pub target: u32,
}

/// How control leaves an instruction.
pub enum EdgeKind {
    /// Follow the target of a conditional branch.
    BranchTaken,
    /// Continue past a conditional branch.
    BranchNotTaken,
}

/// Holds a table of edges.
pub struct EdgeTable;

impl EdgeTable {
    /// Reads the source of an edge.
    pub fn source(&self) -> u32 {
        0
    }
    /// Reads the target of an edge.
    pub fn target(&self) -> u32 {
        0
    }
}
