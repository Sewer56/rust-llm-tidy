//! The move record the reorder pass reports for each relocated item.

use crate::source::ItemKind;
use core::fmt;

/// A single reorder move: one item whose output position differs from its
/// input position.
///
/// Positions are 1-based item sequence positions, matching the user-visible
/// `from pos A to pos B` reporting. This type is deliberately serde-free; the
/// CLI layer is responsible for its own serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct ReorderMove {
    /// 1-based output position of the moved item.
    to: usize,
    /// 1-based input position of the moved item.
    from: usize,
    /// Description of the item that directly follows this one in the reordered
    /// output (the item it lands before), if any.
    before: Option<Box<str>>,
    /// Kind of the moved item (e.g. `fn`, `impl`).
    kind: ItemKind,
    /// Name of the moved item, when it has one.
    name: Option<Box<str>>,
    /// 1-based source line where the moved item starts, used to describe
    /// unnamed items (e.g. impl blocks).
    line: usize,
}

impl ReorderMove {
    /// Assemble a move record from one item's reorder data.
    ///
    /// # Arguments
    ///
    /// - `to` - 1-based output position of the moved item.
    /// - `from` - 1-based input position of the moved item.
    /// - `before` - description of the item this one lands before, if any.
    /// - `kind` - kind of the moved item (e.g. `fn`, `impl`).
    /// - `name` - name of the moved item, when it has one.
    /// - `line` - 1-based source line where the moved item starts.
    ///
    /// # Returns
    ///
    /// The assembled move record.
    pub(super) fn new(
        to: usize,
        from: usize,
        before: Option<Box<str>>,
        kind: ItemKind,
        name: Option<Box<str>>,
        line: usize,
    ) -> Self {
        Self {
            to,
            from,
            before,
            kind,
            name,
            line,
        }
    }

    /// 1-based output position of the moved item.
    ///
    /// # Returns
    ///
    /// The position the item occupies in the reordered output.
    pub fn to(&self) -> usize {
        self.to
    }

    /// 1-based input position of the moved item.
    ///
    /// # Returns
    ///
    /// The position the item occupied in the original input order.
    pub fn from(&self) -> usize {
        self.from
    }

    /// Description of the item this one lands before, when it is not the last
    /// item in the reordered output.
    ///
    /// # Returns
    ///
    /// The following item's description, or `None` when this move's item is
    /// last in the reordered output.
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }

    /// Kind of the moved item.
    ///
    /// # Returns
    ///
    /// A reference to the moved item's [`ItemKind`].
    pub fn kind(&self) -> &ItemKind {
        &self.kind
    }

    /// Name of the moved item, when it has one.
    ///
    /// # Returns
    ///
    /// The item's name, or `None` when the item is unnamed (e.g. an impl
    /// block).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Human-readable rendering of this move, e.g.
    /// `rearrange fn a_main from pos 2 to pos 1 (before b_helper)`.
    ///
    /// The trailing `(before C)` clause is omitted when the item is the last
    /// in the reordered output.
    ///
    /// # Returns
    ///
    /// The formatted message string.
    pub fn message(&self) -> String {
        let subject = match &self.name {
            Some(name) => format!("{} {name}", self.kind),
            None => format!("{} at line {}", self.kind, self.line),
        };
        let mut out = format!(
            "rearrange {subject} from pos {} to pos {}",
            self.from, self.to
        );
        if let Some(before) = &self.before {
            out.push_str(&format!(" (before {before})"));
        }
        out
    }
}

impl fmt::Display for ReorderMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}
