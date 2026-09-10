//! Stable lint codes and selectable operations shared by all languages.

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
    CODE_FORBIDDEN_CHARACTERS,
    CODE_TEST_NAMING,
    CODE_TEST_SUMMARY,
    CODE_MODULE_SIZE,
    CODE_MOD002,
    CODE_QUALIFIED_PATH,
    CODE_LEN001,
    CODE_SYM,
    CODE_DUPLICATION,
];
/// Rule code for placeholder text in doc comments.
pub const CODE_DOC_PLACEHOLDER: &str = "DOC006";
/// Rule code for same-file textual duplication reminders.
pub const CODE_DUPLICATION: &str = "DUP001";
/// Rule code for `# Errors` bullets listing enum variants out of
/// alphabetical order.
pub const CODE_ERROR_VARIANT_ORDER: &str = "DOC008";
/// Rule code for an untagged fenced code block.
pub const CODE_FENCE_TAG: &str = "TEXT005";
/// Rule code for configured forbidden characters in prose.
pub const CODE_FORBIDDEN_CHARACTERS: &str = "TEXT009";
/// Rule code for a misshapen header opener paragraph.
pub const CODE_HEADER_OPENER: &str = "TEXT004";
/// Rule code for a function or method body over its line budget.
pub const CODE_LEN001: &str = "LEN001";
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
/// Rule code for full namespace qualification in code.
pub const CODE_QUALIFIED_PATH: &str = "MOD003";
/// Rule code for an over-limit sentence of measured prose.
pub const CODE_SENTENCE_LENGTH: &str = "TEXT003";
/// Rule code for a configured usage or declaration hint.
pub const CODE_SYM: &str = "SYM";
/// Rule code for a discouraged test-function name.
pub const CODE_TEST_NAMING: &str = "TEST001";
/// Rule code for a test function missing its summary comment.
pub const CODE_TEST_SUMMARY: &str = "TEST002";
/// Rule code for a bullet list exceeding its source-line budget.
pub const CODE_TEXT008: &str = "TEXT008";
/// Rule code for an undocumented parameter.
pub const CODE_UNDOCUMENTED_PARAM: &str = "DOC005";
/// Rule code for a vague `# Errors` section.
pub const CODE_VAGUE_ERRORS: &str = "DOC003";
/// Rule code for a discouraged verbose synonym in measured text.
pub const CODE_VERBOSE_SYNONYMS: &str = "TEXT006";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_codes_should_list_all_registered_codes() {
        // `LINT_CODES` is the source of truth for CLI rule validation. It must
        // enumerate every code produced by the backends and the text rules.
        let expected = [
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
            CODE_FORBIDDEN_CHARACTERS,
            CODE_TEST_NAMING,
            CODE_TEST_SUMMARY,
            CODE_MODULE_SIZE,
            CODE_MOD002,
            CODE_QUALIFIED_PATH,
            CODE_LEN001,
            CODE_SYM,
            CODE_DUPLICATION,
        ];

        assert_eq!(LINT_CODES, expected);
    }
}
