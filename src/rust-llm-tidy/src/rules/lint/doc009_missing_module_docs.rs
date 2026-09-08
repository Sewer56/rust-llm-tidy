//! Shared header-writing guidance for missing Rust and Python module docs.

/// Explain useful header content without imposing extra detection criteria.
pub(crate) const HEADER_GUIDANCE: &str = indoc::indoc! {"
    Why: A purpose-first header helps readers understand the module
    without reading its implementation.

    Suggestions:
    - Read the module and relevant callers; document only supported facts.
    - Start with one concise sentence explaining what the module does and why.
      Do not just restate its name. A simple module needs no more.
    - If more detail is useful, put it below the summary, separated by a blank doc line.
    - Use that detail to outline major responsibilities, entry points, or non-obvious constraints.
    - Use bullets for multiple topics.
    - Link to item docs instead of repeating their details."};

#[cfg(test)]
mod tests {
    use crate::languages::LanguageBackend;
    use crate::languages::python::PythonBackend;
    use crate::rules::lint::CODE_MISSING_MODULE_DOCS;

    #[test]
    fn python_diagnostic_should_explain_why_and_suggest_supported_header_content() {
        let parsed = PythonBackend.parse("value = 1\n").unwrap();

        let diagnostics = PythonBackend.lint(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_MISSING_MODULE_DOCS)
            .unwrap();

        assert!(
            diagnostic
                .message
                .starts_with("module file is missing a module docstring.")
        );
        assert!(diagnostic.message.contains(
            "\n\nWhy: A purpose-first header helps readers understand the module\n\
             without reading its implementation.\n\nSuggestions:\n\
             - Read the module and relevant callers; document only supported facts."
        ));
    }
}
