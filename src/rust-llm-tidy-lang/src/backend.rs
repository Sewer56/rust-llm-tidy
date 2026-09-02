//! The [`LanguageBackend`] contract and the extension registry that resolves
//! a backend per source-file extension.

use crate::rust_backend::RustBackend;
use rust_llm_tidy_model::parse::ParseResult;
use std::cmp::Ordering;

/// Extensions with a registered backend, sorted by extension (ASCII) so
/// binary search applies. The sortedness test guards this invariant.
static BACKED_EXTENSIONS: &[(&str, &dyn LanguageBackend)] = &[("rs", &RUST)];
/// The Rust backend - the one registered parser today.
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
    /// tree-sitter fails to produce a syntax tree. The Rust grammar
    /// error-recovers, so invalid syntax still parses.
    fn parse(&self, source: &str) -> anyhow::Result<ParseResult>;

    /// The AST pipeline ops this backend implements, as pipeline rule names
    /// (`reorder`, `vis`, `lints`).
    ///
    /// Consumers compose this with their admission profiles: an op runs only
    /// when both the profile and the backend's list carry it.
    fn ast_ops(&self) -> &'static [&'static str];
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

    /// Uppercase and mixed-case extensions resolve identically to their
    /// lowercase forms.
    #[test]
    fn lookup_matches_extensions_ascii_case_insensitively() {
        for ext in ["RS", "Rs", "rS"] {
            assert!(backend_for(ext).is_some(), ".{ext} must resolve like .rs");
        }
    }

    /// Extensions without a registered backend resolve no backend, so no AST
    /// op can dispatch for them: code languages, the markdown family, data
    /// formats, unmapped extensions, and the empty extension.
    #[test]
    fn backendless_extensions_resolve_no_backend() {
        for ext in ["cs", "py", "md", "json", "org", ""] {
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
