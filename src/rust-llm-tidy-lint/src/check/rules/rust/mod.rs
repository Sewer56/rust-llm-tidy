//! The Rust item rules: DOC001-DOC006 and TEST001, one pure function per
//! rule over a [`rust_llm_tidy_model::parse::SourceItem`] returning
//! [`Vec<Diagnostic>`].
//!
//! The lang crate's C# backend implements the same codes over its own
//! parse; other languages reach them only through the text rules.

pub use arguments::{missing_arguments_section, undocumented_param};
pub use docs::missing_docs;
pub use errors::{missing_errors_section, vague_errors};
pub use placeholder::doc_placeholder;
use rust_llm_tidy_model::parse::ItemKind;
pub use test_naming::test_naming;

mod arguments;
mod docs;
mod errors;
mod placeholder;
mod test_naming;

/// Documentable items: everything except modules, imports, impls, macros,
/// macro invocations, uncategorized items, and extern crate.
///
/// `Mod` is excluded: `//!` inner docs often live in a file this
/// single-file checker does not parse.
///
/// Used by DOC001 ([`missing_docs`]) and DOC006 ([`doc_placeholder`]).
fn is_documentable(kind: &ItemKind) -> bool {
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

/// The section body: lines after the header at `start` up to the next
/// trimmed `# ` header or end of docs, empty and content lines alike.
///
/// Used by DOC003 ([`vague_errors`]) and DOC005 ([`undocumented_param`]).
fn section_body(docs: &[String], start: usize) -> Vec<&str> {
    docs[start + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|s| !s.trim().starts_with("# "))
        .collect()
}
