//! Parse C# source and expose declaration facts to the rule implementations.
//!
//! The parser emits shared source items with type and namespace members.
//! Lint and ordering policies live under `crate::rules`.
//!
//! The TEXT* text checks ride the same lint composition from
//! [`text_regions`]' doc-region walk of the same parse.
//!
//! [`parse`]: parse::parse
//! [`text_regions`]: text_regions
//!
//! # Reorder degradation
//!
//! The reorder permutation declines a source under any of four conditions:
//!
//! - The parse tree carries error nodes.
//! - The preprocessor region scan rejects the source.
//! - Two top-level declarations share a row.
//! - The text uses CR-styled line endings.
//!
//! An unsafe or unrepresentable construct degrades to a no-op rather
//! than a guessed rewrite.
//!
//! A declined source returns `None`, so callers emit zero change
//! records and never write.

use crate::languages::LanguageBackend;
use crate::reporting::Diagnostic;
use crate::rules::lint::csharp as lints;
use crate::rules::transform::reorder::Permutation;
use crate::source::ParseResult;
pub use analysis::can_throw::CanThrowIndex;

pub mod analysis;
mod lines;
pub(crate) mod parse;
pub(crate) mod regions;
pub(crate) mod text_regions;

/// The `cs` backend.
pub(crate) struct CSharpBackend;

impl LanguageBackend for CSharpBackend {
    fn language(&self) -> anyhow::Result<tree_sitter::Language> {
        c_sharp_language()
    }

    fn parse(&self, source: &str) -> anyhow::Result<ParseResult> {
        parse::parse(source)
    }

    fn ast_ops(&self) -> &'static [&'static str] {
        &["reorder", "lints"]
    }

    fn lint(&self, parsed: &ParseResult) -> Vec<Diagnostic> {
        lints::run(parsed)
    }

    fn lint_indexed(&self, parsed: &ParseResult, index: &CanThrowIndex) -> Vec<Diagnostic> {
        lints::run_indexed(parsed, Some(index))
    }

    fn reorder_permutation(&self, parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
        crate::rules::transform::reorder::csharp::reorder_permutation(parsed)
    }
}

/// The tree-sitter-c-sharp grammar this backend parses with.
///
/// # Errors
///
/// Returns an error when the bundled grammar cannot convert into a
/// [`tree_sitter::Language`] (cannot happen with the pinned grammar
/// version).
fn c_sharp_language() -> anyhow::Result<tree_sitter::Language> {
    Ok(tree_sitter_c_sharp::LANGUAGE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both entry points consume the same MOD003 diagnostics.
    #[test]
    fn lint_entry_points_should_run_the_same_mod003_composition() {
        let repeated = concat!(
            "class C {\n",
            "    void M() {\n",
            "        System.Console.WriteLine(1);\n",
            "        System.Console.WriteLine(2);\n",
            "        System.Console.WriteLine(3);\n",
            "    }\n",
            "}\n",
        );
        let parsed = parse::parse(repeated).unwrap();

        let via_lint = CSharpBackend.lint(&parsed);
        let via_indexed = CSharpBackend.lint_indexed(&parsed, &CanThrowIndex::default());

        assert_eq!(via_indexed, via_lint);
        assert_eq!(
            via_lint
                .iter()
                .filter(|d| d.code == crate::rules::lint::CODE_QUALIFIED_PATH)
                .count(),
            3,
            "MOD003 must fire through every dispatch shape"
        );
    }
}
