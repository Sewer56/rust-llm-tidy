//! Documentation checks over measured text: the shared lint-code registry
//! and the TEXT001/TEXT002 text rules.
//!
//! This module carries the registry every language's findings draw from
//! ([`LINT_CODES`] plus the internal `CODE_TITLES` table).
//!
//! The text pipeline: [`run_text_checks`] and [`run_region_checks`]
//! measure text through `plaintext` and run the text rules over the
//! result.
//!
//! The AST item rules (DOC001-DOC006, TEST001) live with their languages:
//! the `lints` module of each backend in `rust_llm_tidy-lang` implements
//! the codes over its own parse.
//!
//! # Structure
//!
//! - `plaintext` - measures text or doc regions into a `Document`.
//! - `rules` - the rules that construct diagnostics:
//!   - `rules::text` - TEXT001/TEXT002 over a measured document.
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
//! | `DOC006`  | Warning  | A doc comment contains placeholder text (`TODO`/`FIXME`/`TBD`).        |
//! | `TEXT001` | Error    | A doc paragraph over 240 chars of full text.                           |
//! | `TEXT002` | Warning  | A doc line over 80 chars of full text.                                 |
//! | `TEST001` | Warning  | A `#[test]` fn uses a `test_*` or `case_*` name, not a behavioral one. |
//!
//! The DOC and TEST codes are emitted by the lang crate's backends; the
//! TEXT codes by this module's text rules. Each code also carries a short
//! friendly title; [`Diagnostic::title`] exposes it for a finding.

use crate::diagnostic::Diagnostic;
pub use plaintext::{Dialect, DocRegion, RegionLine, line_marker_regions};

mod plaintext;
mod rules;

/// Friendly title per lint code, paired `(code, title)` in [`LINT_CODES`]
/// order.
///
/// Titles are the short human-readable names output consumers render next
/// to the code; they are static so a lookup allocates nothing.
pub(crate) const CODE_TITLES: &[(&str, &str)] = &[
    (CODE_MISSING_DOCS, "missing documentation"),
    (CODE_MISSING_ERRORS, "missing `# Errors` section"),
    (CODE_VAGUE_ERRORS, "vague `# Errors` section"),
    (CODE_MISSING_ARGUMENTS, "missing `# Arguments` section"),
    (CODE_UNDOCUMENTED_PARAM, "undocumented parameter"),
    (CODE_DOC_PLACEHOLDER, "placeholder text"),
    (CODE_PARAGRAPH_SIZE, "oversized paragraph"),
    (CODE_LINE_LENGTH, "long line"),
    (CODE_TEST_NAMING, "non-behavioral test name"),
];
/// All lint codes accepted through `include.rules`, `exclude.rules`,
/// `--include`, and `--exclude`, in the order they run. The CLI validates rule
/// names against this slice plus
/// `KNOWN_FIX_OPS` (defined in the CLI crate's `config` module).
pub const LINT_CODES: &[&str] = &[
    CODE_MISSING_DOCS,
    CODE_MISSING_ERRORS,
    CODE_VAGUE_ERRORS,
    CODE_MISSING_ARGUMENTS,
    CODE_UNDOCUMENTED_PARAM,
    CODE_DOC_PLACEHOLDER,
    CODE_PARAGRAPH_SIZE,
    CODE_LINE_LENGTH,
    CODE_TEST_NAMING,
];
/// Rule code for placeholder text in doc comments.
pub const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for an over-limit stripped doc line.
pub const CODE_LINE_LENGTH: &str = "TEXT002";
/// Rule code for a missing `# Arguments` section.
pub const CODE_MISSING_ARGUMENTS: &str = "DOC004";
/// Rule code for missing doc comments.
pub const CODE_MISSING_DOCS: &str = "DOC001";
/// Rule code for a missing `# Errors` section.
pub const CODE_MISSING_ERRORS: &str = "DOC002";
/// Rule code for an over-limit paragraph of stripped doc text.
pub const CODE_PARAGRAPH_SIZE: &str = "TEXT001";
/// Rule code for a discouraged test-function name.
pub const CODE_TEST_NAMING: &str = "TEST001";
/// Rule code for an undocumented parameter.
pub const CODE_UNDOCUMENTED_PARAM: &str = "DOC005";
/// Rule code for a vague `# Errors` section.
pub const CODE_VAGUE_ERRORS: &str = "DOC003";

/// Runs TEXT001 and TEXT002 over explicit doc regions, as produced by an
/// AST backend's doc-region walk instead of the extension's line-marker
/// table. Each region is measured with its dialect's rules.
///
/// # Arguments
///
/// - `regions` - the file's doc regions, in source order.
///
/// # Returns
///
/// Diagnostics in source order: TEXT001 per over-limit paragraph, then
/// TEXT002 per over-limit line.
pub fn run_region_checks(regions: Vec<DocRegion>) -> Vec<Diagnostic> {
    rules::text::diagnostics(&plaintext::measure(regions))
}

/// Runs TEXT001 and TEXT002 over one file's raw text.
///
/// TEXT001 fires an Error when a plain paragraph's size exceeds 240 chars,
/// and a Warning when a bullet's does; TEXT002 fires a Warning for every
/// line over 80 chars.
///
/// Both count the full line text; table rows, code blocks, and link
/// reference definitions are exempt.
///
/// # Arguments
///
/// - `source` - the raw file text.
/// - `ext` - the file extension, selecting the comment marker table.
///
/// # Returns
///
/// Diagnostics in source order: TEXT001 per over-limit paragraph (bullet
/// warnings after their paragraph position), then TEXT002 per over-limit line.
pub fn run_text_checks(source: &str, ext: &str) -> Vec<Diagnostic> {
    rules::text::diagnostics(&plaintext::analyze(source, ext))
}

/// Friendly title for `code`, or `None` when `code` is not a lint code.
pub(crate) fn title_for_code(code: &str) -> Option<&'static str> {
    CODE_TITLES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, title)| *title)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns only the diagnostics with the given code.
    ///
    /// Shared by the rule-family and dialect submodule tests.
    pub(crate) fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    #[test]
    fn lint_codes_lists_all_nine_codes() {
        // `LINT_CODES` is the source of truth for CLI rule validation. It must
        // enumerate every code produced by the backends and the text rules.
        assert_eq!(
            LINT_CODES.len(),
            9,
            "LINT_CODES must list exactly nine codes: {LINT_CODES:?}"
        );
        for code in [
            CODE_MISSING_DOCS,
            CODE_MISSING_ERRORS,
            CODE_VAGUE_ERRORS,
            CODE_MISSING_ARGUMENTS,
            CODE_UNDOCUMENTED_PARAM,
            CODE_DOC_PLACEHOLDER,
            CODE_PARAGRAPH_SIZE,
            CODE_LINE_LENGTH,
            CODE_TEST_NAMING,
        ] {
            assert!(LINT_CODES.contains(&code), "LINT_CODES is missing {code}");
        }
    }

    /// The title table pairs every lint code with a non-empty title and
    /// holds no extra codes.
    #[test]
    fn code_titles_cover_exactly_the_nine_lint_codes() {
        assert_eq!(
            CODE_TITLES.len(),
            LINT_CODES.len(),
            "CODE_TITLES must pair exactly the nine lint codes"
        );
        for code in LINT_CODES {
            let title =
                title_for_code(code).unwrap_or_else(|| panic!("no title defined for {code}"));
            assert!(!title.is_empty(), "title for {code} must not be empty");
        }
    }
}
