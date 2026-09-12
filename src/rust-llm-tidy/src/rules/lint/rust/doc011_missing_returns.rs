//! `DOC011` - missing `# Returns` section on public functions that return
//! a documented-value type.
//!
//! [`check`] fires on public functions returning a value
//! ([`ReturnKind::Value`], warning) or exactly `bool`
//! ([`ReturnKind::Bool`], reminder) when their docs lack a `# Returns`
//! header.

use super::RETURNS_HEADERS;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_RETURNS;
use crate::source::{ReturnKind, SourceItem, VisibilityTier};

/// Message for a `bool`-returning `pub fn` without a `# Returns` section.
const BOOL_MESSAGE: &str = "Function returns `bool` but has no `# Returns` section.\n\n\
     Why: Readers should not have to inspect the implementation to understand what `true` and `false` mean.\n\n\
     Suggestions:\n\
     - No change is needed if the function name or summary already makes both outcomes clear.\n\
     - Otherwise, consider a short `# Returns` section explaining when the function returns `true` and when it returns `false`.";
/// Message for a value-returning `pub fn` without a `# Returns` section.
const VALUE_MESSAGE: &str = "Function returns a value but has no `# Returns` section.\n\n\
     Why: Readers need to understand what the returned value represents, not just its type.\n\n\
     Suggestions:\n\
     - Add a `# Returns` section explaining what the value represents and any special cases callers need to handle.\n\
     - Describe only existing behavior. Do not invent guarantees or change the implementation to satisfy this lint.";

/// `DOC011` - `pub fn` returning a documented-value type must have a
/// `# Returns` section.
///
/// Fires on fully-public functions (`pub fn`) whose docs lack a
/// `# Returns` header when the return type carries a documented value.
/// Severity follows the kind: [`ReturnKind::Value`] warns and
/// [`ReturnKind::Bool`] reminds.
///
/// Trivial return kinds never fire: unit, never, no declared type,
/// `Self`, and `Result<(), _>`.
///
/// # Arguments
///
/// - `item` - the parsed source item to inspect for a missing `# Returns`
///   section on a `pub fn` with a documented-value return type.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if !item.is_fn() || item.visibility() != Some(VisibilityTier::Pub) {
        return Vec::new();
    }
    let (severity, message) = match item.return_kind() {
        ReturnKind::Value => (Severity::Warning, VALUE_MESSAGE),
        ReturnKind::Bool => (Severity::Reminder, BOOL_MESSAGE),
        ReturnKind::NoValue | ReturnKind::SelfValue | ReturnKind::ResultUnit => {
            return Vec::new();
        }
    };
    if find_returns_section(item.doc_comments()) {
        return Vec::new();
    }

    vec![Diagnostic {
        title: Some("missing `# Returns` section".into()),
        severity,
        code: CODE_MISSING_RETURNS,
        message: message.to_string(),
        line: item.start_line(),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

/// True when `docs` contain a `# Returns`/`# Return` section header
/// (case-insensitive on trimmed lines).
fn find_returns_section(docs: &[String]) -> bool {
    docs.iter().any(|d| {
        RETURNS_HEADERS
            .iter()
            .any(|h| d.trim().eq_ignore_ascii_case(h))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse;
    use crate::rules::lint::rust::tests::parse_one;
    use rstest::rstest;

    // ── DOC011: missing returns section ──

    // pub fn returning a value, no # Returns section -> warning with exact message.
    #[test]
    fn check_should_warn_when_value_fn_has_no_returns_section() {
        let item = parse_one("/// Doubles.\npub fn double(x: u32) -> u32 { x * 2 }");

        let diags = check(&item);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_RETURNS);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[0].message, VALUE_MESSAGE);
    }

    // pub fn returning bool, no # Returns section -> reminder with exact message.
    #[test]
    fn check_should_remind_when_bool_fn_has_no_returns_section() {
        let item =
            parse_one("/// Checks emptiness.\npub fn is_empty(s: &str) -> bool { s.is_empty() }");

        let diags = check(&item);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Reminder);
        assert_eq!(diags[0].message, BOOL_MESSAGE);
    }

    // Any recognized header alias suppresses the diagnostic.
    #[rstest]
    #[case::returns("# Returns")]
    #[case::return_alias("# Return")]
    // Case-insensitivity
    #[case::lowercase("# returns")]
    #[case::uppercase("# RETURNS")]
    #[case::mixed("# rEtUrN")]
    fn check_should_stay_quiet_when_any_header_variant_documents_returns(#[case] header: &str) {
        let source = format!(
            "/// Doubles.\n///\n/// {header}\n///\n/// The doubled value.\npub fn double() -> u32 {{ 2 }}"
        );
        let item = parse_one(&source);

        assert!(
            check(&item).is_empty(),
            "header `{header}` should suppress DOC011"
        );
    }

    // Unrecognized header (# Output) still triggers the diagnostic.
    #[test]
    fn test_missing_returns_rejects_unknown_header() {
        let item = parse_one(
            "/// Doubles.\n///\n/// # Output\n///\n/// The doubled value.\npub fn double() -> u32 { 2 }",
        );

        assert_eq!(
            check(&item).len(),
            1,
            "`# Output` is not a recognized returns header"
        );
    }

    // ── DOC011: skipped return kinds and visibilities ──

    // Unit/never/no-type returns never fire on free fns.
    #[rstest]
    #[case::no_type("/// Docs.\npub fn unit() {}")]
    #[case::unit("/// Docs.\npub fn unit_explicit() -> () {}")]
    #[case::never("/// Docs.\npub fn never() -> ! { loop {} }")]
    fn check_should_stay_quiet_when_return_kind_is_trivial(#[case] source: &str) {
        let item = parse_one(source);

        assert!(
            check(&item).is_empty(),
            "trivial return should stay DOC011-quiet: {source}"
        );
    }

    // `Self` and `&mut Self` impl methods never fire.
    #[test]
    fn test_missing_returns_skips_self_returns() {
        let source = "/// Docs.\npub struct S;\nimpl S {\n    /// Docs.\n    pub fn new() -> Self { S }\n    /// Docs.\n    pub fn clear(&mut self) -> &mut Self { self }\n}";
        let parsed = parse::parse_source(source).unwrap();
        let members = parse::impl_member_items(&source, parsed.syntax_tree());

        assert!(!members.is_empty());
        assert!(members.iter().all(|m| check(m).is_empty()));
    }

    // `Result<(), E>` never fires, even with # Errors but no # Returns.
    #[test]
    fn test_missing_returns_skips_result_unit() {
        let item = parse_one(
            "/// Docs.\n///\n/// # Errors\n///\n/// Fails always.\npub fn fallible() -> Result<(), String> { Err(String::new()) }",
        );

        assert!(check(&item).is_empty());
    }

    // DOC011 skips private and `pub(crate)` fns.
    #[rstest]
    #[case::private("/// Docs.\nfn private() -> u32 { 2 }")]
    #[case::restricted("/// Docs.\npub(crate) fn restricted() -> u32 { 2 }")]
    fn check_should_stay_quiet_when_fn_is_not_public(#[case] source: &str) {
        let item = parse_one(source);

        assert!(
            check(&item).is_empty(),
            "non-pub fn should be skipped: {source}"
        );
    }
}
