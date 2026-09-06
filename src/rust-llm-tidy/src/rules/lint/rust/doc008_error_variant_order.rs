//! `DOC008` - `# Errors` variants listed out of alphabetical order.
//!
//! [`check`] fires when an `# Errors` section lists the returned in-crate
//! enum's variants in an order other than alphabetical.

use super::{find_errors_section, is_pub_result_fn, section_body};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_ERROR_VARIANT_ORDER;
use crate::source::{ItemKind, ParseResult, SourceItem};

/// `DOC008` - `# Errors` variants must be listed in alphabetical order.
///
/// Fires on `pub fn` returning `Result` when all of the following hold:
///
/// - a `# Errors` section exists on the item;
/// - the error type's final path segment resolves to a top-level enum in
///   the same file;
/// - the section's `[`Enum::Variant`]` links (short-reference form,
///   path-qualified prefixes accepted) list that enum's variants in an
///   order that decreases under Rust `str` ordering.
///
/// Links to other enums, prose, and unresolved or non-enum error types
/// never participate.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for variant ordering.
/// - `enum_names` - sorted names of the file's top-level enums
///   (see [`top_level_enum_names`]).
pub(super) fn check(item: &SourceItem, enum_names: &[&str]) -> Vec<Diagnostic> {
    if !is_pub_result_fn(item) {
        return Vec::new();
    }

    // Same-file resolution: the error type's final segment must name a
    // top-level enum here; everything else is exempt (D3).
    let Some(error_enum) = item.result_error_type() else {
        return Vec::new();
    };
    if enum_names.binary_search(&error_enum).is_err() {
        return Vec::new();
    }

    let Some(start) = find_errors_section(item.doc_comments()) else {
        return Vec::new();
    };

    let body = section_body(item.doc_comments(), start);
    let variants: Vec<&str> = body
        .iter()
        .flat_map(|line| linked_variants(line, error_enum))
        .collect();
    if is_non_decreasing(&variants) {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Error,
        code: CODE_ERROR_VARIANT_ORDER,
        message: format!("`# Errors` lists variants of `{error_enum}` out of alphabetical order"),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

/// Sorted names of the file's top-level enums, for same-file resolution.
///
/// The returned slice is sorted so membership queries can binary-search.
pub(super) fn top_level_enum_names(parsed: &ParseResult) -> Vec<&str> {
    let mut names: Vec<&str> = parsed
        .items
        .iter()
        .filter(|it| it.kind() == &ItemKind::Enum)
        .filter_map(|it| it.name())
        .collect();
    names.sort_unstable();
    names
}

/// True when `variants` never decreases under Rust `str` ordering.
fn is_non_decreasing(variants: &[&str]) -> bool {
    variants.windows(2).all(|pair| pair[0] <= pair[1])
}

/// Variants of `enum_name` linked on `line`, in document order.
///
/// A participating link is a bracketed `[`path::Enum::Variant`]` reference
/// whose second-to-last `::` segment is `enum_name`; the yielded variant is
/// the last segment. Bracket contents without a `::` path are ignored.
fn linked_variants<'a>(line: &'a str, enum_name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    line.split(']').filter_map(move |chunk| {
        let inner = chunk.rsplit_once('[')?.1;
        link_variant(inner, enum_name)
    })
}

/// The variant named by link content `inner` when it targets `enum_name`.
///
/// Surrounding backticks are trimmed first, so backticked forms like
/// `` [`Enum::Variant`] `` and `` [`path::Enum::Variant`] `` participate
/// like their plain counterparts.
fn link_variant<'a>(inner: &'a str, enum_name: &str) -> Option<&'a str> {
    let inner = inner.trim_matches('`');
    let mut segments = inner.rsplit("::");
    let variant = segments.next()?;
    let linked_enum = segments.next()?;
    (linked_enum == enum_name).then_some(variant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;

    /// Run the DOC008 rule over every item of `source`, as `run_all` does.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed = parse_source(source).unwrap();
        let enum_names = top_level_enum_names(&parsed);
        parsed
            .items
            .iter()
            .flat_map(|item| check(item, &enum_names))
            .collect()
    }

    /// Fixture: an in-crate `Error` enum plus a documented `load` fn whose
    /// `# Errors` body is `errors_body`.
    fn documented_fn(errors_body: &str) -> String {
        format!(
            "/// Doc.\npub enum Error {{ NotFound, Denied }}\n/// Loads.\n///\n/// # Errors\n///\n{errors_body}\npub fn load() -> Result<(), Error> {{ Ok(()) }}\n"
        )
    }

    // ── DOC008: error variant order ──

    // Variants listed out of alphabetical order -> error.
    #[test]
    fn fires_when_variants_out_of_order() {
        let source = documented_fn("/// Returns [Error::NotFound] then [Error::Denied].");
        let diags = lint(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_ERROR_VARIANT_ORDER);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("Error"));
    }

    // Alphabetical listing (equal neighbors allowed) -> silent.
    #[test]
    fn silent_when_variants_alphabetical() {
        let source = documented_fn("/// Returns [Error::Denied] then [Error::NotFound].");
        assert!(lint(&source).is_empty());

        // Non-decreasing: a repeated variant does not break the order.
        let source = documented_fn("/// Returns [Error::Denied] then [Error::Denied].");
        assert!(lint(&source).is_empty());
    }

    // Out-of-crate error type (`std::io::Error`) -> exempt even when its
    // final segment collides with no local enum.
    #[test]
    fn silent_when_error_type_out_of_crate() {
        let source = "\
/// Doc.\npub enum Other {{ A, B }}\n\
/// Loads.\n\
///\n/// # Errors\n///\n\
/// Returns [Error::B] then [Error::A].\n\
pub fn load() -> Result<(), std::io::Error> { Ok(()) }\n";
        assert!(lint(source).is_empty());
    }

    // Non-enum error type (a struct) -> exempt.
    #[test]
    fn silent_when_error_type_is_not_an_enum() {
        let source = "\
/// Doc.\npub struct Wrapper;\n\
/// Loads.\n\
///\n/// # Errors\n///\n\
/// Returns [Wrapper::B] then [Wrapper::A].\n\
pub fn load() -> Result<(), Wrapper> { Ok(()) }\n";
        assert!(lint(source).is_empty());
    }

    // Path-qualified links participate like plain ones.
    #[test]
    fn fires_when_path_qualified_links_out_of_order() {
        let source =
            documented_fn("/// Returns [crate::Error::NotFound] then [crate::Error::Denied].");
        assert_eq!(lint(&source).len(), 1);
    }

    // Interleaved prose and links to other enums never participate.
    #[test]
    fn ignores_prose_and_other_enum_links() {
        let source = documented_fn(
            "/// Fails hard sometimes.\n/// See [Other::Z] for context.\n/// Returns [Error::Denied] then [Error::NotFound].",
        );
        // The two participating links are alphabetical -> silent.
        assert!(lint(&source).is_empty());

        let source = documented_fn(
            "/// Returns [Error::NotFound].\n/// See [Other::A] between.\n/// Then [Error::Denied].",
        );
        assert_eq!(lint(&source).len(), 1);
    }

    // Backticked short form participates; out of order -> error.
    #[test]
    fn fires_when_backticked_links_out_of_order() {
        let source = documented_fn("/// Returns [`Error::NotFound`] then [`Error::Denied`].");
        assert_eq!(lint(&source).len(), 1);
    }

    // Backticked and plain links compare by bare variant names.
    #[test]
    fn silent_when_backticked_and_plain_links_alphabetical() {
        let source = documented_fn("/// Returns [`crate::Error::Denied`] then [Error::Denied].");
        assert!(lint(&source).is_empty());
    }

    // No participating links at all -> not applicable.
    #[test]
    fn silent_when_no_variant_links() {
        let source = documented_fn("/// Returns an error if loading fails.");
        assert!(lint(&source).is_empty());
    }
}
