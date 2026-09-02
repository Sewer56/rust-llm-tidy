//! The Rust backend: a passthrough over the model crate's tree-sitter-rust
//! parse setup.

use crate::backend::LanguageBackend;
use rust_llm_tidy_model::parse::{self, ParseResult};

/// The `rs` backend.
///
/// Wraps the parse setup the pipeline has always used for Rust - the model
/// crate's tree-sitter-rust grammar and [`parse::parse_source`] - adding no
/// behavior of its own.
pub struct RustBackend;

impl LanguageBackend for RustBackend {
    fn language(&self) -> anyhow::Result<tree_sitter::Language> {
        parse::rust_language()
    }

    fn parse(&self, source: &str) -> anyhow::Result<ParseResult> {
        parse::parse_source(source)
    }

    fn ast_ops(&self) -> &'static [&'static str] {
        &["reorder", "vis", "lints"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The passthrough must produce exactly the model crate's parse output:
    /// same items, preamble, and trailer.
    ///
    /// The fixture covers a documented `pub fn` with parameters and a
    /// `Result` return, a trait impl, a test module, and undecorated items,
    /// so compared fields hold non-default values.
    #[test]
    fn parse_matches_the_model_parse_output() {
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
}
