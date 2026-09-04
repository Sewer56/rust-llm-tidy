//! The [`LanguageBackend`] contract and the extension registry that resolves
//! a backend per source-file extension.

use crate::csharp::CSharpBackend;
use crate::python_backend::PythonBackend;
use crate::rust_backend::RustBackend;
use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_model::parse::ParseResult;
use rust_llm_tidy_reorder::reorder::Permutation;
use std::cmp::Ordering;

/// Extensions with a registered backend, sorted by extension (ASCII) so
/// binary search applies. The sortedness test guards this invariant.
static BACKED_EXTENSIONS: &[(&str, &dyn LanguageBackend)] = &[
    ("cs", &CSHARP),
    ("py", &PYTHON),
    ("pyi", &PYTHON),
    ("rs", &RUST),
];
/// The C# backend - the tree-sitter-c-sharp parse setup.
static CSHARP: CSharpBackend = CSharpBackend;
/// The Python backend - the tree-sitter-python parse setup, doc regions
/// only.
static PYTHON: PythonBackend = PythonBackend;
/// The Rust backend - the parser the pipeline has always used for `.rs`.
static RUST: RustBackend = RustBackend;

/// One language's AST parse setup, serving the pipeline's AST ops.
///
/// Backends are stateless statics shared across threads, so the trait
/// requires [`Sync`]. They emit the model crate's item types
/// ([`ParseResult`]), so the reorder and lint passes consume one item shape
/// for every language.
pub trait LanguageBackend: Sync {
    /// The tree-sitter grammar this backend parses with.
    ///
    /// Callers build their own [`tree_sitter::Parser`] and set this language
    /// on it; the backend itself holds no parser state.
    ///
    /// # Errors
    ///
    /// Returns an error when the grammar cannot be constructed into a
    /// [`tree_sitter::Language`] (cannot happen with the bundled Rust
    /// grammar).
    fn language(&self) -> anyhow::Result<tree_sitter::Language>;

    /// Parse `source` into the shared item model.
    ///
    /// # Arguments
    ///
    /// - `source`: the file's text to parse.
    ///
    /// # Errors
    ///
    /// Returns an error when the language's grammar cannot be constructed or
    /// tree-sitter fails to produce a syntax tree.
    ///
    /// Error-recovering grammars still parse invalid syntax; see
    /// [`Self::lint`] and [`Self::reorder_permutation`] for how their
    /// callers treat such trees.
    fn parse(&self, source: &str) -> anyhow::Result<ParseResult>;

    /// The AST pipeline ops this backend implements, as pipeline rule names
    /// (`reorder`, `vis`, `lints`).
    ///
    /// Consumers compose this with their admission profiles: an op runs only
    /// when both the profile and the backend's list carry it.
    fn ast_ops(&self) -> &'static [&'static str];

    /// The lint diagnostics for a parse produced by [`Self::parse`].
    ///
    /// Backends whose grammar error-recovers may return no diagnostics for
    /// trees with error nodes: findings against misread declarations would
    /// be noise.
    fn lint(&self, parsed: &ParseResult) -> Vec<Diagnostic>;

    /// Compute the full reorder permutation for a parse produced by
    /// [`Self::parse`]: the top-level item order plus any in-type member
    /// permutations.
    ///
    /// Returns `Ok(None)` when the source holds constructs the engine
    /// declines to reorder (parse-tree error nodes, unsupported
    /// preprocessor shapes): callers degrade to a no-op with zero change
    /// records instead of guessing a partial rewrite.
    ///
    /// # Errors
    ///
    /// Returns an error on internal engine failure (a malformed graph or
    /// permutation over an already-parsed result).
    fn reorder_permutation(&self, parsed: &ParseResult) -> anyhow::Result<Option<Permutation>>;
}

/// The registered backend for `ext`, ASCII case-insensitively (`.RS` resolves
/// like `.rs`).
///
/// Extensions without a registered backend resolve `None` and can run no AST
/// ops.
///
/// # Arguments
///
/// - `ext`: a path extension without the leading dot; an empty string
///   resolves to `None`.
#[inline]
pub fn backend_for(ext: &str) -> Option<&'static dyn LanguageBackend> {
    BACKED_EXTENSIONS
        .binary_search_by(|probe| cmp_ext(probe.0, ext))
        .ok()
        .map(|i| BACKED_EXTENSIONS[i].1)
}

/// ASCII case-insensitive ordering, matching the CLI admission registry's
/// extension comparisons.
#[inline]
fn cmp_ext(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rs` resolves to a backend carrying every AST op.
    #[test]
    fn rs_resolves_with_all_ast_ops() {
        let backend = backend_for("rs").expect("rs must resolve to a backend");

        assert_eq!(backend.ast_ops(), ["reorder", "vis", "lints"].as_slice());
    }

    /// `cs` resolves to a backend carrying reorder and lints, never `vis`
    /// (visibility narrowing stays Rust-only).
    #[test]
    fn cs_resolves_with_reorder_and_lints() {
        let backend = backend_for("cs").expect("cs must resolve to a backend");

        assert_eq!(backend.ast_ops(), ["reorder", "lints"].as_slice());
    }

    /// `py`/`pyi` resolve to the Python backend, which carries no AST
    /// ops: its parse serves the docstring text checks only.
    #[test]
    fn py_resolves_with_no_ast_ops() {
        for ext in ["py", "pyi", "PY"] {
            let backend = backend_for(ext).expect("py must resolve to a backend");

            let no_ops: [&str; 0] = [];
            assert_eq!(backend.ast_ops(), no_ops, ".{ext}: no AST ops");
        }
    }

    /// Uppercase and mixed-case extensions resolve identically to their
    /// lowercase forms.
    #[test]
    fn lookup_matches_extensions_ascii_case_insensitively() {
        for (upper, lower) in [("RS", "rs"), ("Cs", "cs"), ("cS", "cs")] {
            assert!(
                backend_for(upper).is_some(),
                ".{upper} must resolve like .{lower}"
            );
        }
    }

    /// Extensions without a registered backend resolve no backend, so no AST
    /// op can dispatch for them: code languages, the markdown family, data
    /// formats, unmapped extensions, and the empty extension.
    #[test]
    fn backendless_extensions_resolve_no_backend() {
        for ext in ["js", "md", "json", "org", ""] {
            assert!(backend_for(ext).is_none(), ".{ext} must resolve no backend");
        }
    }

    /// Binary search requires the table's sortedness.
    #[test]
    fn backend_table_stays_sorted_for_binary_search() {
        for pair in BACKED_EXTENSIONS.windows(2) {
            assert!(
                cmp_ext(pair[0].0, pair[1].0) == Ordering::Less,
                "`{}` must sort before `{}`",
                pair[0].0,
                pair[1].0
            );
        }
    }
}
