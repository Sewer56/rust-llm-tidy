//! Stable lint codes and diagnostic titles shared by all languages.

/// Friendly title per lint code, paired `(code, title)` in [`LINT_CODES`]
/// order.
///
/// Titles are the short human-readable names output consumers render next
/// to the code; they are static so a lookup allocates nothing.
pub(crate) const CODE_TITLES: &[(&str, &str)] = &[
    (CODE_MISSING_DOCS, "missing documentation"),
    (
        CODE_MISSING_MODULE_DOCS,
        "module file without top-level docs",
    ),
    (CODE_MISSING_ERRORS, "missing `# Errors` section"),
    (CODE_VAGUE_ERRORS, "vague `# Errors` section"),
    (
        CODE_ERROR_VARIANT_ORDER,
        "error variants out of alphabetical order",
    ),
    (CODE_MISSING_ARGUMENTS, "missing `# Arguments` section"),
    (CODE_UNDOCUMENTED_PARAM, "undocumented parameter"),
    (CODE_DOC_PLACEHOLDER, "placeholder text"),
    (CODE_PARAGRAPH_SIZE, "oversized paragraph"),
    (CODE_LINE_LENGTH, "long line"),
    (CODE_SENTENCE_LENGTH, "long sentence"),
    (CODE_HEADER_OPENER, "header opener shape"),
    (CODE_FENCE_TAG, "untagged fenced code block"),
    (CODE_VERBOSE_SYNONYMS, "verbose synonym"),
    (CODE_PASSIVE_NARRATION, "passive construction"),
    (CODE_TEXT008, "dense bullet list"),
    (CODE_TEST_NAMING, "non-behavioral test name"),
    (CODE_MODULE_SIZE, "oversized module"),
    (CODE_MOD002, "fn-local `use` without `#[cfg]`"),
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
    CODE_MISSING_MODULE_DOCS,
    CODE_MISSING_ERRORS,
    CODE_VAGUE_ERRORS,
    CODE_ERROR_VARIANT_ORDER,
    CODE_MISSING_ARGUMENTS,
    CODE_UNDOCUMENTED_PARAM,
    CODE_DOC_PLACEHOLDER,
    CODE_PARAGRAPH_SIZE,
    CODE_LINE_LENGTH,
    CODE_SENTENCE_LENGTH,
    CODE_HEADER_OPENER,
    CODE_FENCE_TAG,
    CODE_VERBOSE_SYNONYMS,
    CODE_PASSIVE_NARRATION,
    CODE_TEXT008,
    CODE_TEST_NAMING,
    CODE_MODULE_SIZE,
    CODE_MOD002,
];
/// Rule code for placeholder text in doc comments.
pub const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for `# Errors` bullets listing enum variants out of
/// alphabetical order.
pub const CODE_ERROR_VARIANT_ORDER: &str = "DOC008";
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
/// Rule code for a module file without top-level docs.
pub const CODE_MISSING_MODULE_DOCS: &str = "DOC009";
/// Rule code for a `use` inside a function body without its own
/// `#[cfg]` attribute.
pub const CODE_MOD002: &str = "MOD002";
/// Rule code for a source file over its language's line budget.
pub const CODE_MODULE_SIZE: &str = "MOD001";
/// Rule code for an over-limit paragraph of stripped doc text.
pub const CODE_PARAGRAPH_SIZE: &str = "TEXT001";
/// Rule code for passive constructions and past-behavior narration.
pub const CODE_PASSIVE_NARRATION: &str = "TEXT007";
/// Rule code for an over-limit sentence of measured prose.
pub const CODE_SENTENCE_LENGTH: &str = "TEXT003";
/// Rule code for a discouraged test-function name.
pub const CODE_TEST_NAMING: &str = "TEST001";
/// Rule code for a bullet list exceeding its source-line budget.
pub const CODE_TEXT008: &str = "TEXT008";
/// Rule code for an undocumented parameter.
pub const CODE_UNDOCUMENTED_PARAM: &str = "DOC005";
/// Rule code for a vague `# Errors` section.
pub const CODE_VAGUE_ERRORS: &str = "DOC003";
/// Rule code for a discouraged verbose synonym in measured text.
pub const CODE_VERBOSE_SYNONYMS: &str = "TEXT006";

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
    fn lint_codes_lists_all_nineteen_codes() {
        // `LINT_CODES` is the source of truth for CLI rule validation. It must
        // enumerate every code produced by the backends and the text rules.
        assert_eq!(
            LINT_CODES.len(),
            19,
            "LINT_CODES must list exactly nineteen codes: {LINT_CODES:?}"
        );
        for code in [
            CODE_MISSING_DOCS,
            CODE_MISSING_MODULE_DOCS,
            CODE_MISSING_ERRORS,
            CODE_VAGUE_ERRORS,
            CODE_ERROR_VARIANT_ORDER,
            CODE_MISSING_ARGUMENTS,
            CODE_UNDOCUMENTED_PARAM,
            CODE_DOC_PLACEHOLDER,
            CODE_PARAGRAPH_SIZE,
            CODE_LINE_LENGTH,
            CODE_SENTENCE_LENGTH,
            CODE_HEADER_OPENER,
            CODE_FENCE_TAG,
            CODE_VERBOSE_SYNONYMS,
            CODE_PASSIVE_NARRATION,
            CODE_TEXT008,
            CODE_TEST_NAMING,
            CODE_MODULE_SIZE,
            CODE_MOD002,
        ] {
            assert!(LINT_CODES.contains(&code), "LINT_CODES is missing {code}");
        }
    }

    /// The title table pairs every lint code with a non-empty title and
    /// holds no extra codes.
    #[test]
    fn code_titles_cover_exactly_the_lint_codes() {
        assert_eq!(
            CODE_TITLES.len(),
            LINT_CODES.len(),
            "CODE_TITLES must pair exactly the lint codes"
        );
        for code in LINT_CODES {
            let title =
                title_for_code(code).unwrap_or_else(|| panic!("no title defined for {code}"));
            assert!(!title.is_empty(), "title for {code} must not be empty");
        }
    }
}
