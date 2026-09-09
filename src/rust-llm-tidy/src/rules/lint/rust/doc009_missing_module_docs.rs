//! `DOC009` - module file without top-level docs.
//!
//! [`check`] fires once per file that has top-level items but no `//!`
//! module-doc line in its preamble. `///` outer docs on the first item
//! do not count, and item-less files never fire.
//!
//! The diagnostic guides header writing and module-root navigation, not code moves.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_MODULE_DOCS;
use crate::rules::lint::doc009_missing_module_docs::HEADER_GUIDANCE;
use crate::source::ParseResult;

/// `DOC009` - module files must carry `//!` top-level docs.
///
/// Fires once when all the following hold:
///
/// - the file has at least one top-level item;
/// - no line of the file preamble starts (after whitespace) with `//!`.
///
/// The preamble is the source before the first top-level item, bounded by
/// [`ParseResult::preamble_end`]. That boundary sits after any `///` docs
/// and attributes attached to the first item, so outer docs never
/// satisfy the check.
///
/// `//!` lines after the first item fall outside the preamble. Files with
/// no top-level items are exempt: there is no module content to
/// document.
///
/// # Arguments
///
/// - `parsed` - the parsed source result whose preamble is inspected for
///   a `//!` module-doc line.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    // Malformed syntax makes item spans and `preamble_end` unreliable;
    // skip doc checking rather than fire on a garbage boundary.
    if parsed.syntax_tree().root_node().has_error() {
        return Vec::new();
    }
    if parsed.items.is_empty() {
        return Vec::new();
    }
    let has_module_docs = parsed.source[..parsed.preamble_end]
        .lines()
        .any(|line| line.trim_start().starts_with("//!"));
    if has_module_docs {
        return Vec::new();
    }

    vec![Diagnostic {
        title: Some("module file without top-level docs".into()),
        severity: Severity::Error,
        code: CODE_MISSING_MODULE_DOCS,
        message: indoc::formatdoc! {"
            module file is missing `//!` module docs.

            {HEADER_GUIDANCE}
            - Add `//!` docs before the first top-level item.
            - For a module root (`mod.rs`, or `foo.rs` with child modules), identify
              main entry points and relevant child-module responsibilities.
            - Keep this change to header writing; do not move code."},
        line: 1,
        item_kind: "file".to_string(),
        item_name: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;

    /// Run the DOC009 rule over `source`, as `run_all` does.
    fn lint(source: &str) -> Vec<Diagnostic> {
        check(&parse_source(source).unwrap())
    }

    // ── DOC009: module file without top-level docs ──

    // Items with no module docs -> one error at the file's first line.
    // Metadata and section structure are pinned here.
    #[test]
    fn fires_when_items_have_no_module_docs() {
        let diags = lint("pub fn load() {}\n");

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MISSING_MODULE_DOCS);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].item_kind, "file");
        assert_eq!(diags[0].item_name, None);
        assert!(
            diags[0]
                .message
                .starts_with("module file is missing `//!` module docs.\n\nWhy:")
        );
        assert!(diags[0].message.contains("\n\nSuggestions:\n"));
        assert!(
            diags[0]
                .message
                .contains("- Add `//!` docs before the first top-level item.")
        );
    }

    // `///` outer docs attach to the first item, not the preamble.
    #[test]
    fn fires_when_only_outer_docs_on_first_item() {
        let diags = lint("/// Loads the thing.\npub fn load() {}\n");
        assert_eq!(diags.len(), 1);
    }

    // A `//!` line in the preamble satisfies detection.
    #[test]
    fn silent_when_preamble_carries_module_docs() {
        assert!(lint("//! Loads records.\npub fn load() {}\n").is_empty());
    }

    // `#![...]` inner attributes precede the module docs -> still satisfied.
    #[test]
    fn silent_when_module_docs_follow_inner_attributes() {
        let source = "#![allow(dead_code)]\n//! Loads records.\npub fn load() {}\n";
        assert!(lint(source).is_empty());
    }

    // Item-less file -> exempt; there is no module content to document.
    #[test]
    fn silent_when_file_has_no_items() {
        let source = "// Only a comment.\n#![allow(dead_code)]\n";
        assert!(lint(source).is_empty());
    }

    // A `//!` line after the first item sits outside the preamble.
    #[test]
    fn fires_when_module_doc_lookalike_follows_first_item() {
        let source = "fn a() {}\n//! lookalike\nfn b() {}\n";
        assert_eq!(lint(source).len(), 1);
    }

    // An indented `//!` line still counts as a module doc.
    #[test]
    fn silent_when_module_doc_line_is_indented() {
        let source = "    //! Loads records.\npub fn load() {}\n";
        assert!(lint(source).is_empty());
    }

    // A `//!` lookalike inside a plain comment is not a module doc.
    #[test]
    fn fires_when_lookalike_sits_inside_plain_comment() {
        let source = "// //! not a module doc\npub fn load() {}\n";
        assert_eq!(lint(source).len(), 1);
    }

    // Parse errors make spans unreliable; never fire on broken syntax.
    #[test]
    fn silent_when_syntax_is_malformed() {
        let source = "pub fn load() {}\nfn broken( {}\n";
        assert!(lint(source).is_empty());
    }

    // `#![doc = ...]` carries no module-doc credit.
    #[test]
    fn fires_when_doc_attribute_carries_module_doc_text() {
        let source = "#![doc = \"//! Loads records.\"]\npub fn load() {}\n";
        assert_eq!(lint(source).len(), 1);
    }
}
