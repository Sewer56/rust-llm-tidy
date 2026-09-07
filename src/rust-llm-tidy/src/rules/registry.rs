//! Stable lint codes and diagnostic titles shared by all languages.

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
    (CODE_SENTENCE_LENGTH, "long sentence"),
    (CODE_HEADER_OPENER, "header opener shape"),
    (CODE_FENCE_TAG, "untagged fenced code block"),
    (CODE_TEST_NAMING, "non-behavioral test name"),
];
/// Selectable transformations and the lint group, in pipeline order.
pub const KNOWN_FIX_OPS: &[&str] = &["tables", "fences", "links", "reorder", "vis", "lints"];
/// All lint codes accepted through `include.rules`, `exclude.rules`,
/// `--include`, and `--exclude`, in the order they run.
///
/// The CLI validates rule names against this slice plus
/// `KNOWN_FIX_OPS` in the library's `config` module.
pub const LINT_CODES: &[&str] = &[
    CODE_MISSING_DOCS,
    CODE_MISSING_ERRORS,
    CODE_VAGUE_ERRORS,
    CODE_MISSING_ARGUMENTS,
    CODE_UNDOCUMENTED_PARAM,
    CODE_DOC_PLACEHOLDER,
    CODE_PARAGRAPH_SIZE,
    CODE_LINE_LENGTH,
    CODE_SENTENCE_LENGTH,
    CODE_HEADER_OPENER,
    CODE_FENCE_TAG,
    CODE_TEST_NAMING,
];
/// Rule code for placeholder text in doc comments.
pub const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for an untagged fenced code block.
pub const CODE_FENCE_TAG: &str = "TEXT005";
/// Rule code for a misshapen header opener paragraph.
pub const CODE_HEADER_OPENER: &str = "TEXT004";
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
/// Rule code for an over-limit sentence of measured prose.
pub const CODE_SENTENCE_LENGTH: &str = "TEXT003";
/// Rule code for a discouraged test-function name.
pub const CODE_TEST_NAMING: &str = "TEST001";
/// Rule code for an undocumented parameter.
pub const CODE_UNDOCUMENTED_PARAM: &str = "DOC005";
/// Rule code for a vague `# Errors` section.
pub const CODE_VAGUE_ERRORS: &str = "DOC003";

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

    #[test]
    fn lint_codes_lists_all_twelve_codes() {
        // `LINT_CODES` is the source of truth for CLI rule validation. It must
        // enumerate every code produced by the backends and the text rules.
        assert_eq!(
            LINT_CODES.len(),
            12,
            "LINT_CODES must list exactly twelve codes: {LINT_CODES:?}"
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
            CODE_SENTENCE_LENGTH,
            CODE_HEADER_OPENER,
            CODE_FENCE_TAG,
            CODE_TEST_NAMING,
        ] {
            assert!(LINT_CODES.contains(&code), "LINT_CODES is missing {code}");
        }
    }

    /// The title table pairs every lint code with a non-empty title and
    /// holds no extra codes.
    #[test]
    fn code_titles_cover_exactly_the_twelve_lint_codes() {
        assert_eq!(
            CODE_TITLES.len(),
            LINT_CODES.len(),
            "CODE_TITLES must pair exactly the twelve lint codes"
        );
        for code in LINT_CODES {
            let title =
                title_for_code(code).unwrap_or_else(|| panic!("no title defined for {code}"));
            assert!(!title.is_empty(), "title for {code} must not be empty");
        }
    }
}
