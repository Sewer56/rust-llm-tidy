//! The Rust item lint rules: DOC001-DOC006 and TEST001.
//!
//! One module per rule, named by lint code: [`doc001_missing_docs`]
//! through [`test001_test_naming`]. Each rule is a pure function over a
//! [`SourceItem`] returning [`Vec<Diagnostic>`]; [`run_all`] runs every
//! rule over every item in code order.
//!
//! The C# backend's `lints` module implements the same codes over its own
//! parse; both consume the shared code constants from
//! [`rust_llm_tidy_lint::check`].
//!
//! [`SourceItem`]: rust_llm_tidy_model::parse::SourceItem

use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, SourceItem, VisibilityTier};

mod doc001_missing_docs;
mod doc002_missing_errors_section;
mod doc003_vague_errors;
mod doc004_missing_arguments;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod test001_test_naming;

/// Accepted rustdoc headers for documenting function parameters.
///
/// All variants are matched case-insensitively, so `# Arguments`, `# arguments`,
/// and `# ARGUMENTS` are equivalent.
const ARGUMENTS_HEADERS: &[&str] = &[
    "# Arguments",
    "# Argument",
    "# Parameters",
    "# Parameter",
    "# Params",
    "# Param",
];

/// Run every Rust item rule over `parsed` and return all diagnostics.
///
/// Diagnostics are returned in source order (by item, then by rule in
/// code order: DOC001-DOC006, then TEST001). The returned `Vec` is empty
/// when every item passes every rule.
///
/// # Arguments
///
/// - `parsed` - the parsed source result whose items are checked.
pub(super) fn run_all(parsed: &ParseResult) -> Vec<Diagnostic> {
    // Each item produces at most a handful of diagnostics; preallocating to the
    // item count can reduce regrowth on the common dirty-file path.
    let mut diags = Vec::with_capacity(parsed.items.len());
    for item in &parsed.items {
        diags.extend(doc001_missing_docs::check(item));
        diags.extend(doc002_missing_errors_section::check(item));
        diags.extend(doc003_vague_errors::check(item));
        diags.extend(doc004_missing_arguments::check(item));
        diags.extend(doc005_undocumented_param::check(item));
        diags.extend(doc006_placeholder::check(item));
        diags.extend(test001_test_naming::check(item));
    }
    diags
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric, non-underscore character (or
/// the start/end of the text), so the needle matches when framed by
/// punctuation but never inside a longer word, and `name` matches in
/// `` `name` `` but not in `filename`.
///
/// Used by DOC005 ([`doc005_undocumented_param`]) and DOC006
/// ([`doc006_placeholder`]).
fn contains_word(haystack: &str, needle: &str) -> bool {
    let h = haystack.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    let mut start = 0;
    while let Some(pos) = h[start..].find(&n) {
        let abs = start + pos;
        let before_ok = h[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after_idx = abs + n.len();
        let after_ok = h[after_idx..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if before_ok && after_ok {
            return true;
        }
        start = abs + n.len();
    }
    false
}

/// Index into `doc_comments` of a parameter-documentation header, if present.
///
/// Accepts the common rustdoc variants `# Arguments`, `# Parameters`, and
/// `# Params` (plus their singulars), matched case-insensitively.
///
/// Used by DOC004 ([`doc004_missing_arguments`]) and DOC005
/// ([`doc005_undocumented_param`]).
fn find_arguments_section(docs: &[String]) -> Option<usize> {
    docs.iter().position(|d| {
        let t = d.trim().to_ascii_lowercase();
        ARGUMENTS_HEADERS
            .iter()
            .any(|h| t == h.to_ascii_lowercase())
    })
}

/// Index into `doc_comments` of the `# Errors` section header, if present.
///
/// Used by DOC002 ([`doc002_missing_errors_section`]) and DOC003
/// ([`doc003_vague_errors`]).
fn find_errors_section(docs: &[String]) -> Option<usize> {
    docs.iter()
        .position(|d| d.trim().eq_ignore_ascii_case("# errors"))
}

/// Documentable items: everything except modules, imports, impls, macros,
/// macro invocations, uncategorized items, and extern crate.
///
/// `Mod` is excluded: `//!` inner docs often live in a file this
/// single-file checker does not parse.
///
/// Used by DOC001 ([`doc001_missing_docs`]) and DOC006
/// ([`doc006_placeholder`]).
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

/// True when `item` is a `pub fn` that declares at least one named parameter
/// (the `self` receiver does not count).
///
/// Used by DOC004 ([`doc004_missing_arguments`]) and DOC005
/// ([`doc005_undocumented_param`]).
fn is_pub_fn_with_params(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && !item.params().is_empty()
}

/// True when `item` is a `pub fn` whose return type ends in `Result`.
///
/// Used by DOC002 ([`doc002_missing_errors_section`]) and DOC003
/// ([`doc003_vague_errors`]).
fn is_pub_result_fn(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && item.returns_result()
}

/// The section body: lines after the header at `start` up to the next
/// trimmed `# ` header or end of docs, empty and content lines alike.
///
/// Used by DOC003 ([`doc003_vague_errors`]) and DOC005
/// ([`doc005_undocumented_param`]).
fn section_body(docs: &[String], start: usize) -> Vec<&str> {
    docs[start + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|s| !s.trim().starts_with("# "))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::backends::rust::parse;
    use rust_llm_tidy_model::parse::SourceItem;

    /// Parse `source` and return its first [`SourceItem`].
    ///
    /// Shared by every rule module's `#[cfg(test)] mod tests` so the parser
    /// fixture helper is defined exactly once rather than duplicated.
    pub(super) fn parse_one(source: &str) -> SourceItem {
        let parsed = parse::parse_source(source).unwrap();
        parsed
            .items
            .into_iter()
            .next()
            .expect("expected at least one item")
    }
}
