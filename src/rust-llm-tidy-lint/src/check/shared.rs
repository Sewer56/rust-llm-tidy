//! Helpers shared by more than one documentation check.
//!
//! [`is_documentable`] is used by DOC001 ([`crate::check::missing_docs`]) and
//! DOC006 ([`crate::check::doc_placeholder`]); [`section_body`] is used by
//! DOC003 ([`crate::check::vague_errors`]) and DOC005
//! ([`crate::check::undocumented_param`]). Every other check helper stays
//! private to its owning module.

use rust_llm_tidy_model::parse::ItemKind;

/// Items that should be documented (everything except modules, imports,
/// impls, macros, macro invocations, uncategorized items, and extern crate).
///
/// `Mod` is excluded: modules are documented via `//!` inner docs that often
/// live in a separate file this single-file checker does not parse, so flagging
/// a bare `pub mod foo;` declaration would be a false positive.
pub(crate) fn is_documentable(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Fn
            | ItemKind::Struct
            | ItemKind::Enum
            | ItemKind::Union
            | ItemKind::Type
            | ItemKind::Trait
            | ItemKind::Const
            | ItemKind::Static
    )
}

/// Lines belonging to a doc section body: everything after the header at
/// `start` up to the next `# ` section header or end of docs.
///
/// A section ends at any trimmed line starting with `# `; empty lines and
/// content lines within the section are retained.
pub(crate) fn section_body(docs: &[String], start: usize) -> Vec<&str> {
    docs[start + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|s| !s.trim().starts_with("# "))
        .collect()
}
