//! Change records returned by file transformations.

use crate::source::ItemKind;
use core::num::NonZeroU32;
use std::fmt;

/// A single edit applied by a transformation.
///
/// Lines are 1-based and non-zero. `None` means the record has no specific line.
/// Records describe the affected region without embedding reconstructed source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Optional 1-based line where the affected entity begins (`None` = no line).
    pub line: Option<NonZeroU32>,

    /// Kind of the affected entity (see [`ChangeKind::as_str`] for the string form).
    pub kind: ChangeKind,

    /// Operation code: `FIX`, `REORDER`, or `VIS`.
    pub code: &'static str,

    /// Stable, human-readable description (never the reconstructed source).
    pub message: Box<str>,

    /// Name of the affected item, when it has one.
    pub name: Option<Box<str>>,
}

/// Typed kind of an affected entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// A parsed source item kind (e.g. `fn`, `struct`).
    Item(ItemKind),

    /// A nested code fence whose delimiter was flipped.
    Fence,

    /// A hoisted inline link.
    Link,

    /// A realigned table.
    Table,

    /// An `extern crate` item whose visibility was narrowed.
    ExternCrate,
}

impl ChangeKind {
    /// Return the stable kind name used by plaintext and JSON output.
    ///
    /// # Returns
    /// An [`ItemKind`] name, or `"fence"`, `"link"`, `"table"`, or `"extern crate"`.
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Item(kind) => kind.as_str(),
            ChangeKind::Fence => "fence",
            ChangeKind::Link => "link",
            ChangeKind::Table => "table",
            ChangeKind::ExternCrate => "extern crate",
        }
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Link records carry no line, so only prefix numbered records.
        if let Some(line) = self.line {
            write!(f, "{line}: ")?;
        }
        match &self.name {
            Some(name) => write!(
                f,
                "success[{}]: {} ({} `{}`)",
                self.code,
                self.message,
                self.kind.as_str(),
                name
            ),
            None => write!(
                f,
                "success[{}]: {} ({})",
                self.code,
                self.message,
                self.kind.as_str()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_should_match_lint_shape() {
        // Arrange
        let named = Change {
            line: NonZeroU32::new(20),
            code: "REORDER",
            message: "rearrange fn a_main from pos 2 to pos 1 (before b_helper)".into(),
            kind: ChangeKind::Item(ItemKind::Fn),
            name: Some("a_main".into()),
        };

        // Act
        let output = named.to_string();

        // Assert
        assert_eq!(
            output,
            "20: success[REORDER]: rearrange fn a_main from pos 2 to pos 1 (before b_helper) (fn `a_main`)"
        );
    }

    #[test]
    fn plaintext_should_omit_name_when_unnamed() {
        // Arrange
        let unnamed = Change {
            line: NonZeroU32::new(3),
            code: "FIX",
            message: "flip nested fence at line 3".into(),
            kind: ChangeKind::Fence,
            name: None,
        };

        // Act
        let output = unnamed.to_string();

        // Assert
        assert_eq!(
            output,
            "3: success[FIX]: flip nested fence at line 3 (fence)"
        );
    }
}
