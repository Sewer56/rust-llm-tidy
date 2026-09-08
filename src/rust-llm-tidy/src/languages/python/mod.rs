//! The Python backend: the tree-sitter-python parse setup for `py` and
//! `pyi` sources.
//!
//! Python supports linting through [`text_regions`], which checks module
//! docstrings and produces text regions from the same parse.
//! Missing-docstring diagnostics guide concise, purpose-first module headers.
//! Reorder declines every source.
//!
//! [`text_regions`]: text_regions

use crate::languages::LanguageBackend;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_MODULE_DOCS;
use crate::rules::lint::doc009_missing_module_docs::HEADER_GUIDANCE;
use crate::rules::lint::run_region_checks;
use crate::rules::transform::reorder::Permutation;
use crate::source::ParseResult;

pub(crate) mod text_regions;

/// The `py`/`pyi` backend - the tree-sitter-python parse setup, `lints`
/// only.
pub(crate) struct PythonBackend;

impl LanguageBackend for PythonBackend {
    fn language(&self) -> anyhow::Result<tree_sitter::Language> {
        text_regions::language()
    }

    fn parse(&self, source: &str) -> anyhow::Result<ParseResult> {
        text_regions::parse(source)
    }

    fn ast_ops(&self) -> &'static [&'static str] {
        &["lints"]
    }

    fn lint(&self, parsed: &ParseResult) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if text_regions::module_doc_missing(parsed) {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: CODE_MISSING_MODULE_DOCS,
                message: indoc::formatdoc! {"
                    module file is missing a module docstring.

                    {HEADER_GUIDANCE}
                    - Add a module docstring as the first statement."},
                line: 1,
                item_kind: "file".to_string(),
                item_name: None,
            });
        }

        diagnostics.extend(run_region_checks(text_regions::doc_regions(parsed)));
        diagnostics
    }

    fn reorder_permutation(&self, _parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
        Ok(None)
    }
}
