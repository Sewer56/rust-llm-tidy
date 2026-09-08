//! The Rust backend: grammar, item parser, reorder profile, visibility
//! narrowing, lint rules, and text regions.

use crate::languages::LanguageBackend;
use crate::reporting::Diagnostic;
use crate::rules::lint::rust as lints;
use crate::rules::transform::reorder::Permutation;
use crate::source::ParseResult;

pub(crate) mod parse;
pub(crate) mod text_regions;

/// The `rs` backend.
///
/// Parses Rust source into shared items and dispatches Rust AST operations.
pub struct RustBackend;

impl LanguageBackend for RustBackend {
    fn language(&self) -> anyhow::Result<tree_sitter::Language> {
        Ok(tree_sitter_rust::LANGUAGE.into())
    }

    fn parse(&self, source: &str) -> anyhow::Result<ParseResult> {
        parse::parse_source(source)
    }

    fn ast_ops(&self) -> &'static [&'static str] {
        &["reorder", "vis", "lints"]
    }

    fn lint(&self, parsed: &ParseResult) -> Vec<Diagnostic> {
        lints::run(parsed)
    }

    fn reorder_permutation(&self, parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
        crate::rules::transform::reorder::rust::reorder_permutation(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The backend produces exactly the local parser's output:
    /// same items, preamble, and trailer.
    ///
    /// The fixture covers a documented `pub fn` with parameters and a
    /// `Result` return. It also covers a trait impl, a test module, and
    /// undecorated items. Compared fields hold non-default values.
    #[test]
    fn parse_should_match_local_parser_output() {
        let source = concat!(
            "//! doc\n",
            "use std::fmt;\n",
            "/// Loads the thing.\n",
            "pub fn load(path: &str) -> anyhow::Result<()> {\n",
            "    Ok(())\n",
            "}\n",
            "impl Foo for Bar {\n",
            "    fn baz() {}\n",
            "}\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn t() {}\n",
            "}\n",
        );

        let via_backend = RustBackend.parse(source).unwrap();
        let direct = parse::parse_source(source).unwrap();

        // Debug carries every consumer-visible field: visibility, doc
        // comments, params, returns_result, test flags, impl target, spans.
        assert_eq!(
            format!("{:?}", via_backend.items),
            format!("{:?}", direct.items)
        );
        assert_eq!(via_backend.preamble_end, direct.preamble_end);
        assert_eq!(via_backend.trailer_start, direct.trailer_start);
    }

    /// The grammar hook builds a working parser: a tree parsed with the
    /// backend's language yields the Rust root node.
    #[test]
    fn language_builds_a_parser_for_rust_source() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&RustBackend.language().unwrap())
            .unwrap();

        let tree = parser.parse("fn a() {}", None).unwrap();

        assert_eq!(tree.root_node().kind(), "source_file");
    }

    /// The lint composition emits file-level findings first, then every
    /// item rule in code order per item, then the text tier.
    ///
    /// The sequence is not line-sorted.
    /// In source, `bare`'s DOC001 at line 6 follows the over-budget
    /// doc line's TEXT002 at line 3. In output, it does not.
    #[test]
    fn lint_orders_item_rules_before_the_text_tier() {
        let source = concat!(
            "pub fn load(path: &str, fmt: &str) -> Result<(), String> { Ok(()) }\n",
            "\n",
            "/// A documented function whose doc line runs past the eighty character budget limit for lines.\n",
            "pub fn documented() {}\n",
            "\n",
            "pub fn bare() {}\n",
        );

        let parsed = parse::parse_source(source).unwrap();
        let order: Vec<(usize, &str)> = RustBackend
            .lint(&parsed)
            .iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.code))
            .collect();

        // Hardcoded on purpose: a derived sequence stays invariant under reorders.
        // The doc line's single-sentence opener stays TEXT004-quiet.
        assert_eq!(
            order,
            [
                (1, "DOC009"),
                (1, "DOC001"),
                (1, "DOC002"),
                (1, "DOC004"),
                (6, "DOC001"),
                (3, "TEXT002"),
            ],
            "file-level DOC009, then item rules in code order per item, then the text tier"
        );
    }

    /// Both entry points consume the same MOD003 diagnostics.
    #[test]
    fn lint_entry_points_should_run_the_same_mod003_composition() {
        let repeated = concat!(
            "fn f() {\n",
            "    std::mem::drop(1);\n",
            "    std::mem::drop(2);\n",
            "    std::mem::drop(3);\n",
            "}\n",
        );
        let parsed = parse::parse_source(repeated).unwrap();

        let via_lint = RustBackend.lint(&parsed);
        let via_indexed =
            RustBackend.lint_indexed(&parsed, &crate::languages::CanThrowIndex::default());

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
