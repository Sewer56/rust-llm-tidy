//! The Python backend: the tree-sitter-python parse setup for `py` and
//! `pyi` sources.
//!
//! Python registers no AST ops - it parses for the DOC007/DOC008 text
//! checks only, sourcing them from [`python_text_regions`]' docstring
//! and `#`-comment walk of the same parse. Reorder declines every
//! source.

use crate::backend::LanguageBackend;
use crate::python_text_regions;
use rust_llm_tidy_model::parse::ParseResult;
use rust_llm_tidy_reorder::reorder::Permutation;

/// The `py`/`pyi` backend - doc regions only, no AST ops.
pub(crate) struct PythonBackend;

impl LanguageBackend for PythonBackend {
    fn language(&self) -> anyhow::Result<tree_sitter::Language> {
        python_text_regions::language()
    }

    fn parse(&self, source: &str) -> anyhow::Result<ParseResult> {
        python_text_regions::parse(source)
    }

    fn ast_ops(&self) -> &'static [&'static str] {
        &[]
    }

    fn lint(&self, parsed: &ParseResult) -> Vec<rust_llm_tidy_lint::Diagnostic> {
        python_text_regions::text_checks(parsed)
    }

    fn reorder_permutation(&self, _parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
        Ok(None)
    }
}
