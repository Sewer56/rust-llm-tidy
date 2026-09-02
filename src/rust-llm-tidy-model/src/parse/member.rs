//! In-type member entries for member reordering.
//!
//! [`TypeMember`] positions one member inside a type body by byte span so
//! the reorder engine can permute members without re-parsing the body.

use crate::parse::kind::ItemKind;

/// One member inside a type body (e.g. a C# class member), positioned by
/// byte span for in-type member reordering.
///
/// Language backends whose reorder profiles enable member reordering emit
/// members alongside the enclosing type item; the Rust parse emits none
/// because Rust reorders top-level items only.
///
/// Spans follow the top-level item rules: they tile the type body
/// back-to-back (each member's `end` is the next member's `start`).
///
/// A member's span carries the blank lines and comments preceding it, so
/// member reordering preserves that whitespace.
#[derive(Debug, Clone)]
pub struct TypeMember {
    /// Byte offset of the start of this member (including pinned comments).
    pub start: usize,
    /// Byte offset of the end of this member, including its trailing
    /// newline.
    pub end: usize,
    /// Preprocessor region id: member reordering permutes only within one
    /// region id, so no member crosses a conditional boundary. `0` for
    /// languages without preprocessor conditionals.
    region: u32,
    /// The kind of this member.
    kind: ItemKind,
    /// The name of this member (if it has one).
    name: Option<String>,
}

impl TypeMember {
    /// Create a member entry.
    ///
    /// # Arguments
    ///
    /// - `start` / `end`: byte span of the member, including pinned
    ///   comments and the trailing newline.
    /// - `region`: preprocessor region id of the member's first line.
    /// - `kind` / `name`: classification and name, mirroring the top-level
    ///   item fields.
    pub fn new(
        start: usize,
        end: usize,
        region: u32,
        kind: ItemKind,
        name: Option<String>,
    ) -> Self {
        Self {
            start,
            end,
            region,
            kind,
            name,
        }
    }

    /// The kind of this member.
    #[inline]
    pub fn kind(&self) -> &ItemKind {
        &self.kind
    }

    /// The name of this member, if any.
    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The preprocessor region id of this member.
    #[inline]
    pub fn region(&self) -> u32 {
        self.region
    }
}
