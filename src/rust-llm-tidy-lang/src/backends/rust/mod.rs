//! The Rust backend owns its grammar, item parser, reorder policy, lint
//! rules, and text regions.

use crate::backends::LanguageBackend;
use profile::RustProfile;
use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_model::parse::ParseResult;
use rust_llm_tidy_reorder::graph;
use rust_llm_tidy_reorder::reorder::Permutation;

mod lints;
mod parse;
mod profile;
pub mod text_regions;

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
        // The Ast text-lint tier rides the backend lint composition:
        // the line-marker regions (`///`, `//!`, `//`) merge with the
        // parse tree's `/** */` and `#[doc = "..."]` doc regions.
        let mut diags = lints::run_all(parsed);
        diags.extend(text_regions::text_checks(parsed));
        diags
    }

    fn reorder_permutation(&self, parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
        let order = graph::compute_order(parsed, &RustProfile)?;
        let permutation = Permutation::new(parsed.items.len(), order)?;
        Ok(Some(permutation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The backend produces exactly the local parser's output:
    /// same items, preamble, and trailer.
    ///
    /// The fixture covers a documented `pub fn` with parameters and a
    /// `Result` return, a trait impl, a test module, and undecorated items,
    /// so compared fields hold non-default values.
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

    /// The lint composition emits every item rule in code order per
    /// item, then the text tier, so the sequence is not line-sorted:
    /// `bare`'s DOC001 at line 6 follows the over-budget doc line's
    /// TEXT002 at line 3 in source but not in output.
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

        // Hardcoded on purpose: deriving the sequence from the
        // composition would leave the assertion invariant under reorders.
        assert_eq!(
            order,
            [
                (1, "DOC001"),
                (1, "DOC002"),
                (1, "DOC004"),
                (6, "DOC001"),
                (3, "TEXT002"),
            ],
            "item rules in code order per item, then the text tier"
        );
    }
}
