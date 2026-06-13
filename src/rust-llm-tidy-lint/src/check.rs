//! Documentation checks that run over a parsed source file.
//!
//! Each check is a pure function over a [`ParseResult`] that returns a
//! [`Vec<Diagnostic>`]. [`run_all`] runs every check and concatenates results.
//!
//! # Checks
//!
//! | Code      | Severity | Fires when                                                             |
//! | --------- | -------- | ---------------------------------------------------------------------- |
//! | `DOC001`  | Error    | A non-private item has no `///` doc comment.                           |
//! | `DOC002`  | Error    | A `pub fn` returning `Result` has no `# Errors` section.               |
//! | `DOC003`  | Warning  | A `# Errors` section names no concrete error variant.                  |
//! | `DOC004`  | Warning  | A `pub fn` with parameters has no `# Arguments` section.               |
//! | `DOC005`  | Warning  | A `# Arguments` section does not mention every parameter name.         |
//! | `DOC006`  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`/...).    |
//! | `TEST001` | Warning  | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |

use crate::diagnostic::{Diagnostic, Severity};
use rust_llm_tidy_model::parse::{ItemKind, ParseResult, SourceItem, VisibilityTier};

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
/// Rule code for placeholder text in doc comments.
pub(crate) const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for a missing `# Arguments` section.
pub(crate) const CODE_MISSING_ARGUMENTS: &str = "DOC004";
/// Rule code for missing doc comments.
pub(crate) const CODE_MISSING_DOCS: &str = "DOC001";
/// Rule code for a missing `# Errors` section.
pub(crate) const CODE_MISSING_ERRORS: &str = "DOC002";
/// Rule code for a discouraged test-function name.
pub(crate) const CODE_TEST_NAMING: &str = "TEST001";
/// Rule code for an undocumented parameter.
pub(crate) const CODE_UNDOCUMENTED_PARAM: &str = "DOC005";
/// Rule code for a vague `# Errors` section.
pub(crate) const CODE_VAGUE_ERRORS: &str = "DOC003";

/// Run every documentation check over `parsed` and return all diagnostics.
///
/// Diagnostics are returned in source order (by item, then by check). The
/// returned `Vec` is empty when every item passes every check.
pub fn run_all(parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for item in &parsed.items {
        diags.extend(missing_docs(parsed.source.as_str(), item));
        diags.extend(missing_errors_section(parsed.source.as_str(), item));
        diags.extend(vague_errors(parsed.source.as_str(), item));
        diags.extend(missing_arguments_section(parsed.source.as_str(), item));
        diags.extend(undocumented_param(parsed.source.as_str(), item));
        diags.extend(doc_placeholder(parsed.source.as_str(), item));
        diags.extend(test_naming(parsed.source.as_str(), item));
    }
    diags
}

// ---------------------------------------------------------------------------
// Check 1 - DOC001: missing doc comments
// ---------------------------------------------------------------------------

/// `DOC006` - doc comments must not contain placeholder text.
///
/// Fires on documentable items whose doc comments contain a placeholder marker
/// (`TODO`, `FIXME`, `TBD`, or `...`). Such markers signal unfinished docs that
/// read as finished API documentation.
pub fn doc_placeholder(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    if !is_documentable(item.kind()) {
        return Vec::new();
    }
    let docs = item.doc_comments();
    if docs.is_empty() {
        return Vec::new();
    }
    if !docs.iter().any(|d| contains_placeholder(d)) {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_DOC_PLACEHOLDER,
        message: "doc comment contains placeholder text (TODO/FIXME/TBD/...)".to_string(),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

// ---------------------------------------------------------------------------
// Check 7 - TEST001: test-function naming
// ---------------------------------------------------------------------------

/// `DOC004` - `pub fn` with parameters must have an `# Arguments` section.
///
/// Fires on fully-public functions (`pub fn`) that declare at least one named
/// parameter (excluding `self`) and whose doc comments contain no `# Arguments`
/// or `# Parameters` header.
pub fn missing_arguments_section(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_fn_with_params(item) {
        return Vec::new();
    }
    if find_arguments_section(item.doc_comments()).is_some() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_MISSING_ARGUMENTS,
        message: "pub fn with parameters is missing a `# Arguments` doc section".to_string(),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

// ---------------------------------------------------------------------------
// Check 5 - DOC005: undocumented parameter
// ---------------------------------------------------------------------------

/// `DOC001` - non-private documentable items must have a `///` doc comment.
///
/// Fires on `pub` and `pub(crate)`/`pub(super)`/`pub(in path)` items of
/// documentable kinds (fn, struct, enum, ...) that have zero leading doc
/// comments. Test modules are skipped.
pub fn missing_docs(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    let Some(vis) = item.visibility() else {
        return Vec::new();
    };
    if vis == VisibilityTier::Private {
        return Vec::new();
    }
    if !is_documentable(item.kind()) {
        return Vec::new();
    }
    if item.is_test_module() {
        return Vec::new();
    }
    if !item.doc_comments().is_empty() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Error,
        code: CODE_MISSING_DOCS,
        message: "non-private item is missing a doc comment".to_string(),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

// ---------------------------------------------------------------------------
// Check 2 - DOC002: missing `# Errors` section
// ---------------------------------------------------------------------------

/// `DOC002` - `pub fn` returning `Result` must have a `# Errors` section.
///
/// Fires on fully-public functions (`pub fn`) whose return type ends in
/// `Result` and whose doc comments contain no `# Errors` header.
pub fn missing_errors_section(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_result_fn(item) {
        return Vec::new();
    }
    if find_errors_section(item.doc_comments()).is_some() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Error,
        code: CODE_MISSING_ERRORS,
        message: "pub fn returning Result is missing a `# Errors` doc section".to_string(),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

// ---------------------------------------------------------------------------
// Check 3 - DOC003: vague `# Errors` section
// ---------------------------------------------------------------------------

/// `TEST001` - test functions should use behavioral names.
///
/// Fires on `#[test]` functions whose names use a discouraged pattern
/// (`test_*`, `case_*`, `test` + digits) instead of a behavioral claim shaped
/// `subject_should_expectation_when_condition`. Behavioral names describe the
/// behavior under test without the redundant `test_` prefix the test module
/// already provides.
pub fn test_naming(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    if !item.is_test_fn() {
        return Vec::new();
    }
    let Some(name) = item.name() else {
        return Vec::new();
    };
    if !is_bad_test_name(name) {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_TEST_NAMING,
        message: format!(
            "test function `{name}` should use a behavioral name \
             (subject_should_expectation_when_condition), not a `test_*` or `case_*` prefix"
        ),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: Some(name.to_string()),
    }]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `DOC005` - `# Arguments` section must mention every parameter name.
///
/// Fires on `pub fn` with parameters when an `# Arguments`/`# Parameters`
/// section exists but at least one parameter name is not mentioned anywhere in
/// the section body.
pub fn undocumented_param(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_fn_with_params(item) {
        return Vec::new();
    }
    let Some(start) = find_arguments_section(item.doc_comments()) else {
        return Vec::new();
    };

    let body = section_body(item.doc_comments(), start);
    let undocumented: Vec<&str> = item
        .params()
        .iter()
        .filter(|p| !body.iter().any(|line| line.contains(p.as_str())))
        .map(String::as_str)
        .collect();

    if undocumented.is_empty() {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_UNDOCUMENTED_PARAM,
        message: format!(
            "parameter(s) not documented in the `# Arguments` section: `{}`",
            undocumented.join("`, `")
        ),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

// ---------------------------------------------------------------------------
// Check 6 - DOC006: placeholder text in doc comments
// ---------------------------------------------------------------------------

/// `DOC003` - `# Errors` section must name concrete error variants.
///
/// Fires on `pub fn` returning `Result` when a `# Errors` section exists but
/// none of its bullets reference a concrete variant (detected by the presence
/// of a rustdoc link `[...]` or a `::` path).
pub fn vague_errors(source: &str, item: &SourceItem) -> Vec<Diagnostic> {
    if !is_pub_result_fn(item) {
        return Vec::new();
    }
    let Some(start) = find_errors_section(item.doc_comments()) else {
        return Vec::new();
    };

    let body = section_body(item.doc_comments(), start);
    if body.is_empty() {
        return Vec::new();
    }
    if section_names_variant(&body) {
        return Vec::new();
    }

    vec![Diagnostic {
        severity: Severity::Warning,
        code: CODE_VAGUE_ERRORS,
        message: "`# Errors` section does not name any concrete error variant".to_string(),
        line: line_of(source, item.start),
        item_kind: item.kind().to_string(),
        item_name: item.name().map(str::to_string),
    }]
}

// ---------------------------------------------------------------------------
// Check 4 - DOC004: missing `# Arguments` section
// ---------------------------------------------------------------------------

/// True when `text` contains a placeholder marker: a whole-word `TODO`,
/// `FIXME`, or `TBD` (case-insensitive), or a literal `...`.
fn contains_placeholder(text: &str) -> bool {
    contains_word(text, "todo")
        || contains_word(text, "fixme")
        || contains_word(text, "tbd")
        || text.contains("...")
}

/// Index into `doc_comments` of a parameter-documentation header, if present.
///
/// Accepts the common rustdoc variants `# Arguments`, `# Parameters`, and
/// `# Params` (plus their singulars), matched case-insensitively.
fn find_arguments_section(docs: &[String]) -> Option<usize> {
    docs.iter().position(|d| {
        let t = d.trim().to_ascii_lowercase();
        ARGUMENTS_HEADERS
            .iter()
            .any(|h| t == h.to_ascii_lowercase())
    })
}

/// Index into `doc_comments` of the `# Errors` section header, if present.
fn find_errors_section(docs: &[String]) -> Option<usize> {
    docs.iter().position(|d| d.trim() == "# Errors")
}

/// True when `name` uses a discouraged test-naming pattern.
///
/// Flags the bare `test` name, the redundant `test_*` / `case_*` prefixes, and
/// `test` immediately followed by digits (`test1`, `test2`). Behavioral names
/// like `should_pass_when_valid` pass.
fn is_bad_test_name(name: &str) -> bool {
    name == "test"
        || name.starts_with("test_")
        || name.starts_with("case_")
        || is_test_plus_digits(name)
}

/// Items that should be documented (everything except modules, imports,
/// impls, macros, macro invocations, uncategorized items, and extern crate).
///
/// `Mod` is excluded: modules are documented via `//!` inner docs that often
/// live in a separate file this single-file checker does not parse, so flagging
/// a bare `pub mod foo;` declaration would be a false positive.
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
fn is_pub_fn_with_params(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && !item.params().is_empty()
}

/// True when `item` is a `pub fn` whose return type ends in `Result`.
fn is_pub_result_fn(item: &SourceItem) -> bool {
    item.is_fn() && item.visibility() == Some(VisibilityTier::Pub) && item.returns_result()
}

/// 1-based line number of `byte_offset` within `source`.
fn line_of(source: &str, byte_offset: usize) -> usize {
    source
        .get(..byte_offset)
        .unwrap_or(source)
        .matches('\n')
        .count()
        + 1
}

/// Lines belonging to a doc section body: everything after the header at
/// `start` up to the next `# ` section header or end of docs.
///
/// A section ends at any trimmed line starting with `# `; empty lines and
/// content lines within the section are retained.
fn section_body(docs: &[String], start: usize) -> Vec<&str> {
    docs[start + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|s| !s.trim().starts_with("# "))
        .collect()
}

/// True when any non-blank line in the section body references a concrete
/// variant via a rustdoc link (`[`) or path separator (`::`).
fn section_names_variant(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let t = line.trim();
        !t.is_empty() && (t.contains('[') || t.contains("::"))
    })
}

/// Case-insensitive whole-word match for `needle` in `haystack`.
///
/// A word boundary is any non-alphanumeric, non-underscore character (or the
/// start/end of the text), so `todo` matches in `// TODO:` but not in
/// `todolist`.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let h = haystack.to_ascii_lowercase();
    let mut start = 0;
    while let Some(pos) = h[start..].find(needle) {
        let abs = start + pos;
        let before_ok = h[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after_idx = abs + needle.len();
        let after_ok = h[after_idx..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if before_ok && after_ok {
            return true;
        }
        start = abs + needle.len();
    }
    false
}

/// True for names like `test1`, `test2` (`test` immediately followed by digits).
fn is_test_plus_digits(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("test") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_model::parse;

    fn parse_one(source: &str) -> SourceItem {
        let parsed = parse::parse_source(source).unwrap();
        parsed
            .items
            .into_iter()
            .next()
            .expect("expected at least one item")
    }

    // ── DOC001: missing_docs ──

    #[test]
    fn test_missing_docs_pub_fn() {
        let item = parse_one("pub fn do_thing() {}");
        let diags = missing_docs("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_DOCS);
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn test_missing_docs_documented() {
        let item = parse_one("/// Does the thing.\npub fn do_thing() {}");
        assert!(missing_docs("", &item).is_empty());
    }

    #[test]
    fn test_missing_docs_private_skipped() {
        let item = parse_one("fn helper() {}");
        assert!(missing_docs("", &item).is_empty());
    }

    #[test]
    fn test_missing_docs_pub_struct() {
        let item = parse_one("pub struct Foo;");
        let diags = missing_docs("", &item);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn test_missing_docs_pub_crate() {
        let item = parse_one("pub(crate) fn internal() {}");
        let diags = missing_docs("", &item);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn test_missing_docs_test_mod_skipped() {
        let source = "#[cfg(test)]\npub mod tests {}";
        let item = parse_one(source);
        assert!(missing_docs(source, &item).is_empty());
    }

    #[test]
    fn test_missing_docs_use_skipped() {
        let item = parse_one("pub use std::io;");
        assert!(missing_docs("", &item).is_empty());
    }

    // ── DOC002: missing_errors_section ──

    #[test]
    fn test_missing_errors_no_section() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        let diags = missing_errors_section("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ERRORS);
    }

    #[test]
    fn test_missing_errors_has_section() {
        let item = parse_one(
            "/// Loads a file.\n///\n/// # Errors\n///\n/// Returns nothing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(missing_errors_section("", &item).is_empty());
    }

    #[test]
    fn test_missing_errors_not_result() {
        let item = parse_one("pub fn load() -> u32 { 0 }");
        assert!(missing_errors_section("", &item).is_empty());
    }

    #[test]
    fn test_missing_errors_private_skipped() {
        let item = parse_one("fn load() -> Result<(), String> { Ok(()) }");
        assert!(missing_errors_section("", &item).is_empty());
    }

    // ── DOC003: vague_errors ──

    #[test]
    fn test_vague_errors_no_variants() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns an error if loading fails.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        let diags = vague_errors("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_VAGUE_ERRORS);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn test_vague_errors_with_variants() {
        let item = parse_one(
            "/// Loads.\n///\n/// # Errors\n///\n/// Returns [Error::NotFound] if missing.\npub fn load() -> Result<(), String> { Ok(()) }",
        );
        assert!(vague_errors("", &item).is_empty());
    }

    #[test]
    fn test_vague_errors_no_section_skipped() {
        let item = parse_one("pub fn load() -> Result<(), String> { Ok(()) }");
        assert!(vague_errors("", &item).is_empty());
    }

    // ── DOC004: missing_arguments_section ──

    #[test]
    fn test_missing_arguments_no_section() {
        let item = parse_one("/// Greets.\npub fn greet(name: &str) {}");
        let diags = missing_arguments_section("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_ARGUMENTS);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn test_missing_arguments_has_section() {
        let item = parse_one(
            "/// Greets.\n///\n/// # Arguments\n///\n/// `name` - the name.\npub fn greet(name: &str) {}",
        );
        assert!(missing_arguments_section("", &item).is_empty());
    }

    #[test]
    fn test_missing_arguments_accepts_all_header_variants() {
        // Every accepted alias should suppress DOC004 when params are documented.
        for header in [
            "# Arguments",
            "# Argument",
            "# Parameters",
            "# Parameter",
            "# Params",
            "# Param",
            // Case-insensitivity
            "# arguments",
            "# ARGUMENTS",
            "# pArAmS",
        ] {
            let source = format!(
                "/// Greets.\n///\n/// {header}\n///\n/// `name` - the name.\npub fn greet(name: &str) {{}}",
            );
            let item = parse_one(&source);
            assert!(
                missing_arguments_section("", &item).is_empty(),
                "header `{header}` should suppress DOC004"
            );
        }
    }

    #[test]
    fn test_missing_arguments_rejects_unknown_header() {
        // A non-recognized header should still trigger DOC004.
        let item = parse_one(
            "/// Greets.\n///\n/// # Inputs\n///\n/// `name` - the name.\npub fn greet(name: &str) {}",
        );
        assert_eq!(
            missing_arguments_section("", &item).len(),
            1,
            "`# Inputs` is not a recognized arguments header"
        );
    }

    #[test]
    fn test_missing_arguments_no_params() {
        let item = parse_one("/// Greets.\npub fn greet() {}");
        assert!(missing_arguments_section("", &item).is_empty());
    }

    #[test]
    fn test_missing_arguments_private_skipped() {
        let item = parse_one("fn greet(name: &str) {}");
        assert!(missing_arguments_section("", &item).is_empty());
    }

    // ── DOC005: undocumented_param ──

    #[test]
    fn test_undocumented_param_missing() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\npub fn build(name: &str, fmt: &str) {}",
        );
        let diags = undocumented_param("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_UNDOCUMENTED_PARAM);
        assert!(diags[0].message.contains("fmt"));
    }

    #[test]
    fn test_undocumented_param_all_documented() {
        let item = parse_one(
            "/// Builds.\n///\n/// # Arguments\n///\n/// `name` - the name.\n/// `fmt` - the format.\npub fn build(name: &str, fmt: &str) {}",
        );
        assert!(undocumented_param("", &item).is_empty());
    }

    #[test]
    fn test_undocumented_param_accepts_header_variants() {
        // DOC005 should detect documented params under any accepted header alias.
        for header in ["# Parameters", "# Params", "# Parameter", "# Param"] {
            let source = format!(
                "/// Builds.\n///\n/// {header}\n///\n/// `name` - the name.\n/// `fmt` - the format.\npub fn build(name: &str, fmt: &str) {{}}",
            );
            let item = parse_one(&source);
            assert!(
                undocumented_param("", &item).is_empty(),
                "header `{header}` should be recognized by DOC005"
            );
        }
    }

    #[test]
    fn test_undocumented_param_no_section() {
        let item = parse_one("/// Builds.\npub fn build(name: &str) {}");
        assert!(undocumented_param("", &item).is_empty());
    }

    // ── DOC006: doc_placeholder ──

    #[test]
    fn test_doc_placeholder_todo() {
        let item = parse_one("/// TODO: implement.\npub fn task() {}");
        let diags = doc_placeholder("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_DOC_PLACEHOLDER);
    }

    #[test]
    fn test_doc_placeholder_fixme() {
        let item = parse_one("/// FIXME: broken.\npub fn task() {}");
        assert_eq!(doc_placeholder("", &item).len(), 1);
    }

    #[test]
    fn test_doc_placeholder_ellipsis() {
        let item = parse_one("/// Something ... here.\npub fn task() {}");
        assert_eq!(doc_placeholder("", &item).len(), 1);
    }

    #[test]
    fn test_doc_placeholder_clean() {
        let item = parse_one("/// A clean doc.\npub fn task() {}");
        assert!(doc_placeholder("", &item).is_empty());
    }

    #[test]
    fn test_doc_placeholder_non_documentable() {
        let item = parse_one("/// TODO.\nimpl Foo {}");
        assert!(doc_placeholder("", &item).is_empty());
    }

    // ── TEST001: test_naming ──

    #[test]
    fn test_naming_test_prefix() {
        let item = parse_one("#[test]\nfn test_foo() {}");
        let diags = test_naming("", &item);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_TEST_NAMING);
    }

    #[test]
    fn test_naming_test_digits() {
        let item = parse_one("#[test]\nfn test1() {}");
        assert_eq!(test_naming("", &item).len(), 1);
    }

    #[test]
    fn test_naming_case_prefix() {
        let item = parse_one("#[test]\nfn case_1() {}");
        assert_eq!(test_naming("", &item).len(), 1);
    }

    #[test]
    fn test_naming_behavioral() {
        let item = parse_one("#[test]\nfn should_pass_when_valid() {}");
        assert!(test_naming("", &item).is_empty());
    }

    #[test]
    fn test_naming_non_test() {
        let item = parse_one("fn helper() {}");
        assert!(test_naming("", &item).is_empty());
    }
}
