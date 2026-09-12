//! The Rust lint rules: DOC*, TEST*, LEN*, and MOD*.
//!
//! One module per rule, named by lint code: [`doc001_missing_docs`]
//! through [`test002_test_summary`].
//!
//! Most rules are pure functions over a [`SourceItem`] returning
//! [`Vec<Diagnostic>`]; [`run_all`] runs every rule in code order.
//!
//! [`mod001_module_size`] is file-level instead: the
//! pipeline runs it from `check_file`, outside [`run_all`].
//! [`mod003_qualified_path`] walks the whole retained tree once per
//! file at the end of [`run_all`].
//!
//! [`len001_method_length`] consumes a config threshold, so the pipeline runs it
//! from `check_file` outside [`run_all`].
//!
//! The C# backend's `lints` module implements the same codes over its own
//! parse; both consume the shared code constants from
//! [`crate::rules::lint`].
//!
//! [`SourceItem`]: crate::source::SourceItem

use super::run_region_checks;
use crate::config::forbidden_character_rule::defaults;
use crate::languages::rust::parse;
use crate::languages::rust::text_regions::{doc_regions, forbidden_character_regions};
use crate::reporting::Diagnostic;
use crate::rules::registry::CODE_FORBIDDEN_CHARACTERS;
use crate::source::{ItemKind, ParseResult, SourceItem, VisibilityTier};
use crate::text::measurement::measure;

mod doc001_missing_docs;
mod doc002_missing_errors_section;
mod doc003_vague_errors;
mod doc004_missing_arguments;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod doc008_error_variant_order;
mod doc009_missing_module_docs;
mod doc010_section_order;
mod doc011_missing_returns;
pub(crate) mod len001_method_length;
pub(crate) mod mod001_module_size;
mod mod002_fn_local_use;
mod mod003_qualified_path;
mod test001_test_naming;
mod test002_test_summary;

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
/// Accepted rustdoc headers for documenting return values.
///
/// All variants match case-insensitively, so `# Returns`, `# returns`,
/// and `# RETURNS` are equivalent.
///
/// Used by DOC011 ([`doc011_missing_returns`]); DOC010 ranks the same
/// vocabulary in its standard order.
const RETURNS_HEADERS: &[&str] = &["# Returns", "# Return"];

/// Run Rust item checks, tree checks, and text checks over one parse.
pub(crate) fn run(parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut diagnostics = run_all(parsed);
    diagnostics.extend(run_region_checks(doc_regions(parsed)));
    diagnostics.retain(|d| d.code != CODE_FORBIDDEN_CHARACTERS);
    diagnostics.extend(super::text::forbidden_characters::diagnostics(
        &measure(forbidden_character_regions(parsed)),
        defaults(),
    ));
    diagnostics.extend(mod002_fn_local_use::check(parsed));
    diagnostics
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric character other than `_` (or
/// the start/end of the text). So the needle matches when framed by
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

/// Run every Rust rule over `parsed` and return all diagnostics.
///
/// File-level diagnostics precede item diagnostics, which follow source
/// order and then rule code order: DOC*, then TEST*. Items include
/// impl-block members merged in source order with the top-level items.
///
/// The returned `Vec` is empty only when the file and every item pass
/// every rule.
///
/// # Arguments
///
/// - `parsed` - the parsed source result whose items and file preamble
///   are checked.
fn run_all(parsed: &ParseResult) -> Vec<Diagnostic> {
    // Each item produces at most a handful of diagnostics; preallocating to the
    // item count can reduce regrowth on the common dirty-file path.
    let mut diags = Vec::with_capacity(parsed.items.len());
    diags.extend(doc009_missing_module_docs::check(parsed));

    // DOC008 resolves the returned enum against same-file top-level enum
    // declarations, so it needs the sibling enums, not just the item under
    // check.
    let enums = doc008_error_variant_order::top_level_enums(parsed);

    // Impl members (methods, associated consts/types) are not top-level
    // items; collect them separately and merge them in source order.
    //
    // The collector excludes test-module members. A test-marked method
    // outside a test module still classifies as a test fn, so the TEST
    // rules check it like a free test fn.
    let members = parse::impl_member_items(&parsed.source, parsed.syntax_tree());
    let mut entries: Vec<&SourceItem> = parsed.items.iter().collect();
    entries.extend(members.iter());
    // Sort by byte start so diagnostics follow source order; the stable
    // sort keeps the merge deterministic.
    entries.sort_by_key(|item| item.start);

    for item in entries {
        diags.extend(doc001_missing_docs::check(item));
        diags.extend(doc002_missing_errors_section::check(item));
        diags.extend(doc003_vague_errors::check(item));
        diags.extend(doc004_missing_arguments::check(item));
        diags.extend(doc005_undocumented_param::check(item));
        diags.extend(doc006_placeholder::check(item));
        diags.extend(doc008_error_variant_order::check(item, &enums));
        diags.extend(doc010_section_order::check(item));
        diags.extend(doc011_missing_returns::check(item));
        diags.extend(test001_test_naming::check(item));
        diags.extend(test002_test_summary::check(item));
    }
    diags.extend(mod003_qualified_path::check(parsed));
    diags
}

/// The section body: lines after the header at `start` up to the next
/// trimmed `# ` header or end of docs. It includes empty and content lines
/// alike.
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
    use super::run_all;
    use crate::languages::rust::parse;
    use crate::rules::lint::{
        CODE_MISSING_ARGUMENTS, CODE_MISSING_DOCS, CODE_MISSING_ERRORS, CODE_TEST_NAMING,
        CODE_TEST_SUMMARY,
    };
    use crate::source::SourceItem;

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

    // ── impl members: the DOC rules over impl blocks ──

    const IMPL_MEMBER_SRC: &str = r#"//! mod docs
/// docs
pub struct S;

impl S {
    /// docs for a
    pub fn a(&self, x: u32) -> u32 {
        x
    }

    pub fn b(&self) -> std::io::Result<()> {
        Ok(())
    }

    fn c(&self) {}
}
"#;

    /// An undocumented `pub fn` impl method is a DOC001 candidate, like a
    /// free function.
    #[test]
    fn run_all_should_flag_missing_docs_when_pub_impl_method_is_undocumented() {
        let parsed = parse::parse_source(IMPL_MEMBER_SRC).unwrap();

        let diags = run_all(&parsed);

        assert!(
            diags
                .iter()
                .any(|d| d.code == CODE_MISSING_DOCS && d.item_name.as_deref() == Some("b"))
        );
    }

    /// A `pub fn` impl method returning `Result` needs an `# Errors` section.
    #[test]
    fn run_all_should_flag_missing_errors_section_when_impl_method_returns_result() {
        let source =
            IMPL_MEMBER_SRC.replace("pub fn b(&self)", "/// docs for b\n    pub fn b(&self)");
        let parsed = parse::parse_source(&source).unwrap();

        let diags = run_all(&parsed);

        assert!(
            diags
                .iter()
                .any(|d| d.code == CODE_MISSING_ERRORS && d.item_name.as_deref() == Some("b"))
        );
    }

    /// A `pub fn` impl method with parameters needs an `# Arguments` section.
    #[test]
    fn run_all_should_flag_missing_arguments_section_when_pub_impl_method_takes_params() {
        let source = IMPL_MEMBER_SRC.replace(
            "pub fn a(&self, x: u32) -> u32 {",
            "/// docs for a.\n    pub fn a(&self, x: u32) -> u32 {",
        );
        let parsed = parse::parse_source(&source).unwrap();

        let diags = run_all(&parsed);

        assert!(
            diags
                .iter()
                .any(|d| d.code == CODE_MISSING_ARGUMENTS && d.item_name.as_deref() == Some("a"))
        );
    }

    /// Private methods and impl members inside test modules stay exempt.
    #[test]
    fn run_all_should_skip_private_and_test_module_impl_members() {
        let source = r#"//! mod docs
/// docs
pub struct S;

#[cfg(test)]
mod tests {
    use super::*;
    impl S {
        pub fn helper(&self) {}
    }
}
"#;
        let parsed = parse::parse_source(source).unwrap();

        let diags = run_all(&parsed);

        assert!(diags.is_empty());
    }

    /// A `#[test]` impl method with a summary comment passes TEST002, like
    /// a documented free test fn; a discouraged name still gets TEST001.
    #[test]
    fn run_all_should_check_test_rules_when_test_method_has_summary_comment() {
        let source = r#"//! mod docs
pub struct S;

impl S {
    // Verifies parsing end to end.
    #[test]
    fn parses() {}

    // Verifies naming checks reach methods.
    #[test]
    fn test_renamed() {}
}
"#;
        let parsed = parse::parse_source(source).unwrap();

        let diags = run_all(&parsed);

        assert!(
            diags
                .iter()
                .any(|d| d.code == CODE_TEST_NAMING
                    && d.item_name.as_deref() == Some("test_renamed"))
        );
        assert!(!diags.iter().any(|d| d.code == CODE_TEST_SUMMARY));
    }

    /// Member diagnostics interleave with top-level diagnostics in source
    /// order.
    #[test]
    fn run_all_should_merge_member_diagnostics_in_source_order() {
        let source = r#"//! mod docs
pub fn first() {}

/// docs
pub struct S;

impl S {
    pub fn mid(&self) {}
}

pub fn last() {}
"#;
        let parsed = parse::parse_source(source).unwrap();

        let diags = run_all(&parsed);

        let flagged: Vec<&str> = diags
            .iter()
            .map(|d| d.item_name.as_deref().unwrap_or_default())
            .collect();
        assert_eq!(flagged, ["first", "mid", "last"]);
        let lines: Vec<usize> = diags.iter().map(|d| d.line).collect();
        assert!(lines.windows(2).all(|w| w[0] <= w[1]));
    }

    /// Trait-impl methods carry no visibility, so no DOC rule fires on them,
    /// whether they are marked with `#[test]`.
    #[test]
    fn run_all_should_skip_trait_impl_methods() {
        let source = r#"//! mod docs
/// docs
pub trait Tr {
    /// docs
    fn m(&self);

    /// docs
    fn t(&self);
}

/// docs
pub struct S;

impl Tr for S {
    fn m(&self) {}

    #[test]
    fn t(&self) {}
}
"#;
        let parsed = parse::parse_source(source).unwrap();

        let diags = run_all(&parsed);

        assert!(diags
            .iter()
            .all(|d| d.item_name.as_deref() != Some("m") && d.item_name.as_deref() != Some("t")));
    }
}
