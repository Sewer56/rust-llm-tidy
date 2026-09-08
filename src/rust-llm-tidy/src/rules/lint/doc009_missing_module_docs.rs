//! Shared header-writing guidance for missing Rust and Python module docs.

/// Explain useful header content without imposing extra detection criteria.
pub(crate) const HEADER_GUIDANCE: &str = indoc::indoc! {"
    Help readers unfamiliar with the codebase understand the module's purpose
    without reading its implementation.
    - Read the module and relevant callers; document only supported facts.
    - Start with one concise sentence explaining what the module does and why.
      Do not just restate its name. A simple module needs no more.
    - If more detail is useful, put it below the summary, separated by a blank
      doc line. Outline major responsibilities, entry points, or non-obvious
      constraints. Use bullets for multiple topics.
    - Link to item docs instead of repeating their details."};
