//! The Rust doc-region producer: DOC007/DOC008 regions for `rs` sources.
//!
//! [`text_checks`] measures three doc sources through one region list:
//!
//! - the line-marker regions the plaintext checks have always measured
//!   (`///`, `//!`, and `//` line-comment runs),
//! - outer `/** */` block doc comments, with the block doc dialect,
//! - `#[doc = "..."]` attribute values, whose lines measure like the
//!   same text after `///`.
//!
//! The last two come from the same parse the backend already requires,
//! so nothing re-parses. String literals and code are never comment or
//! attribute nodes, so their content is never measured.
//!
//! An attribute value's text splits into fragments around escape
//! sequences; every fragment measures and the escapes themselves count
//! as nothing.
//!
//! Regions never merge across sources: each block or attribute doc is
//! its own region, so the line-comment output stays unchanged and the
//! additions carry findings only for their own prose.
//!
//! [`DocRegion`]: rust_llm_tidy_lint::check::DocRegion

use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_lint::check::{
    Dialect, DocRegion, RegionLine, line_marker_regions, run_region_checks,
};
use rust_llm_tidy_model::parse::{ParseResult, doc_attribute_content, is_outer_doc};

/// One measured doc node from the tree walk.
enum DocNode<'a> {
    /// The `doc` content child of an outer `/** */` block comment.
    Block(tree_sitter::Node<'a>),
    /// A `#[doc = "..."]` attribute: the `attribute_item` for the
    /// run-continuation gap check and the value's `string_content`.
    Attr {
        item: tree_sitter::Node<'a>,
        content: tree_sitter::Node<'a>,
    },
}

/// Runs the DOC007/DOC008 text checks over `parsed`'s doc prose: the
/// line-comment regions plus the parse tree's block and attribute doc
/// regions, in source order.
///
/// # Arguments
///
/// - `parsed` - the `rs` parse whose doc regions are measured.
///
/// # Returns
///
/// Diagnostics in source order: DOC007 per over-limit paragraph, then
/// DOC008 per over-limit line.
pub fn text_checks(parsed: &ParseResult) -> Vec<Diagnostic> {
    run_region_checks(doc_regions(parsed))
}

/// The file's doc regions in source order: the `rs` line-marker regions
/// merged with the parse tree's block-doc and doc-attribute regions.
fn doc_regions(parsed: &ParseResult) -> Vec<DocRegion> {
    merge_regions(
        line_marker_regions(&parsed.source, "rs"),
        ast_doc_regions(parsed),
    )
}

/// The parse tree's outer block doc comments and doc attributes as
/// regions, in document order.
fn ast_doc_regions(parsed: &ParseResult) -> Vec<DocRegion> {
    let source = parsed.source.as_str();
    let mut docs = Vec::new();
    collect_doc_nodes(parsed.syntax_tree().root_node(), source, &mut docs);

    // Each doc node starts at most one region, so this is the exact
    // upper bound.
    let mut regions: Vec<DocRegion> = Vec::with_capacity(docs.len());
    // End byte of the previous doc attribute's `attribute_item`, for
    // the run-continuation gap check.
    let mut prev_attr_end: Option<usize> = None;
    for doc in docs {
        match doc {
            DocNode::Block(content) => {
                prev_attr_end = None;
                regions.push(block_doc_region(content, source));
            }
            DocNode::Attr { item, content } => {
                // Doc attributes keep one region while nothing but
                // whitespace separates them: consecutive `#[doc]` lines
                // measure as one paragraph, like `///` runs.
                //
                // A trailing attribute (`#[doc = "..."] fn f() {}`)
                // shares its row with item code, so the next item's
                // attribute never joins it.
                let row = content.start_position().row;
                let continues = prev_attr_end
                    .is_some_and(|end| source[end..item.start_byte()].trim().is_empty())
                    && regions.last().is_some_and(|region| {
                        region.dialect == Dialect::Markdown
                            && row <= region.lines.last().map_or(0, |line| line.number)
                    });
                if continues {
                    let region = regions.last_mut().expect("continues implies a region");
                    region.lines.extend(attribute_lines(content, source));
                } else {
                    regions.push(DocRegion {
                        dialect: Dialect::Markdown,
                        lines: attribute_lines(content, source),
                    });
                }
                prev_attr_end = Some(item.end_byte());
            }
        }
    }
    regions
}

/// Merges the line-marker regions with the tree regions into one
/// source-ordered list; both inputs are already ordered by first line.
///
/// An empty input returns the other list unchanged: most files carry
/// no block or attribute docs, so the merge allocates nothing for them.
///
/// A tie keeps the line-marker region first, and no tie can occur: a
/// line-comment region and a block or attribute region never share a
/// first line.
fn merge_regions(marker: Vec<DocRegion>, tree: Vec<DocRegion>) -> Vec<DocRegion> {
    if tree.is_empty() {
        return marker;
    }
    if marker.is_empty() {
        return tree;
    }
    let mut merged = Vec::with_capacity(marker.len() + tree.len());
    let mut tree = tree.into_iter().peekable();
    for region in marker {
        while tree
            .peek()
            .is_some_and(|next| first_line(next) < first_line(&region))
        {
            merged.push(tree.next().expect("peeked region exists"));
        }
        merged.push(region);
    }
    merged.extend(tree);
    merged
}

/// One doc attribute's measured lines: the value's text split on its
/// literal newlines, each line keeping its original number. The
/// conventional leading space goes with the quote, so `#[doc = "
/// text"]` measures like `/// text`.
///
/// The value's text arrives in `string_content` fragments split by
/// escape sequences; every fragment measures and the escapes
/// themselves count as nothing.
fn attribute_lines(content: tree_sitter::Node<'_>, source: &str) -> Vec<RegionLine> {
    content_fragments(content)
        .flat_map(|fragment| content_lines(fragment, source))
        .map(|(number, text)| {
            let text = text.strip_prefix(' ').unwrap_or(text);
            let indented = text.starts_with('\t') || text.starts_with("    ");
            RegionLine {
                number,
                text: text.to_string(),
                indented,
            }
        })
        .collect()
}

/// One outer block doc comment as a block-doc region: the `doc` child's
/// text split into lines with original numbers, each trimmed to its
/// prose. The block doc dialect then strips the `*` continuations and
/// exempts tagged and indented lines.
fn block_doc_region(content: tree_sitter::Node<'_>, source: &str) -> DocRegion {
    DocRegion {
        dialect: Dialect::BlockDoc,
        lines: content_lines(content, source)
            .map(|(number, text)| RegionLine {
                number,
                text: text.trim().to_string(),
                // The dialect derives indented examples after `*`-stripping.
                indented: false,
            })
            .collect(),
    }
}

/// Collects the doc-bearing nodes in document order on one reused
/// cursor.
fn collect_doc_nodes<'a>(root: tree_sitter::Node<'a>, source: &str, out: &mut Vec<DocNode<'a>>) {
    let mut cursor = root.walk();
    'walk: loop {
        let node = cursor.node();
        match node.kind() {
            // Only outer `/** */` docs measure; inner `/*! */` docs and
            // plain `/* */` comments are not doc regions.
            "block_comment" if is_outer_doc(node) => {
                if let Some(content) = node.child_by_field_name("doc") {
                    out.push(DocNode::Block(content));
                }
            }
            "attribute_item" => {
                if let Some(content) = doc_attribute_content(node, source) {
                    out.push(DocNode::Attr {
                        item: node,
                        content,
                    });
                }
            }
            _ => {}
        }
        if cursor.goto_first_child() {
            continue 'walk;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == root {
                return;
            }
        }
    }
}

/// The region's first line number.
fn first_line(region: &DocRegion) -> usize {
    region.lines.first().map_or(0, |line| line.number)
}

/// The node and its later named siblings that are also
/// `string_content` nodes: one string literal's text fragments around
/// its escape sequences.
fn content_fragments<'a>(
    first: tree_sitter::Node<'a>,
) -> impl Iterator<Item = tree_sitter::Node<'a>> {
    let mut next = Some(first);
    std::iter::from_fn(move || {
        while let Some(node) = next {
            next = node.next_named_sibling();
            if node.kind() == "string_content" {
                return Some(node);
            }
        }
        None
    })
}

/// The content node's text as `(1-based line number, line text)` pairs,
/// numbered from the node's own row: a value or block spanning lines
/// keeps each line's original number.
///
/// Lazy, so callers consume each borrowed line once with no
/// intermediate collection.
fn content_lines<'a>(
    content: tree_sitter::Node<'a>,
    source: &'a str,
) -> impl Iterator<Item = (usize, &'a str)> {
    let row = content.start_position().row;
    content
        .utf8_text(source.as_bytes())
        .into_iter()
        .flat_map(move |text| {
            text.split_inclusive('\n')
                .enumerate()
                .map(move |(idx, seg)| {
                    let seg = seg.strip_suffix('\n').unwrap_or(seg);
                    let seg = seg.strip_suffix('\r').unwrap_or(seg);
                    (row + idx + 1, seg)
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_lint::Severity;
    use rust_llm_tidy_lint::check::{CODE_LINE_LENGTH, CODE_PARAGRAPH_SIZE, run_text_checks};

    /// Parses `source` as Rust and runs its text checks.
    fn checks(source: &str) -> Vec<Diagnostic> {
        text_checks(&rust_llm_tidy_model::parse::parse_source(source).unwrap())
    }

    /// The diagnostics carrying `code`.
    fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    /// A 69-char prose line: four of them join past the 240 paragraph
    /// budget while each stays under the 80 line budget.
    fn prose_line() -> String {
        "word ".repeat(14).trim().to_string()
    }

    // ── Line-comment parity ──

    /// Without block or attribute docs the producer is the line-marker
    /// producer: identical diagnostics, so `///`/`//!`/`//` output is
    /// unchanged. The source fires both codes, so the equality compares
    /// real findings, not two empty vectors.
    #[test]
    fn line_comment_sources_match_the_line_marker_checks() {
        let line = prose_line();
        let long = "w".repeat(81);
        let source = format!(
            "// plain note\n\
             //! inner doc\n\
             /// {line}\n\
             /// {line}\n\
             /// {line}\n\
             /// {line}\n\
             /// {long}\n\
             let gap = 1;\n\
             /// after the gap\n\
             ///   indented code line\n\
             fn hidden() {{}}\n"
        );

        let parsed = rust_llm_tidy_model::parse::parse_source(&source).unwrap();
        let producer = text_checks(&parsed);
        assert!(
            !producer.is_empty(),
            "the parity source must fire DOC007 and DOC008"
        );
        assert_eq!(
            producer,
            run_text_checks(&source, "rs"),
            "line-comment-only sources must measure identically"
        );
    }

    /// Attribute docs never join adjacent line-comment paragraphs: the
    /// interleaved runs stay separate regions, so an over-budget join
    /// never forms.
    #[test]
    fn attribute_docs_do_not_join_line_comment_paragraphs() {
        let line = prose_line();
        let source = format!(
            "/// {line}\n#[doc = \" {line}\"]\n/// {line}\n#[doc = \" {line}\"]\nfn f() {{}}\n"
        );

        assert!(
            line.chars().count() * 4 + 3 > 240,
            "joined, the runs must pass the paragraph budget"
        );
        assert!(checks(&source).is_empty(), "separate runs stay quiet");
    }

    // ── Block docs ──

    /// Over-budget `/** */` prose errors with DOC007 at the block's
    /// first prose line, `*` continuations stripped from the count.
    #[test]
    fn outer_block_doc_prose_errors_at_its_first_line() {
        let line = prose_line();
        let source = format!("/**\n * {line}\n * {line}\n * {line}\n * {line}\n */\nfn f() {{}}\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 2, "the first prose line, not the opener");
    }

    /// An 81-char block doc line warns with DOC008 on the measured
    /// length: the `*` continuation never counts.
    #[test]
    fn over_long_block_doc_line_warns_doc008() {
        let long = "b".repeat(81);
        let source = format!("/** prose\n * {long}\n */\nfn f() {{}}\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(
            found[0].message.starts_with("line is 81 chars long."),
            "the `*` continuation must not count: {}",
            found[0].message
        );
    }

    /// Inner `/*! */` docs, plain `/* */` comments, and `#![doc = "..."]`
    /// inner attributes stay unmeasured: over-budget prose in them
    /// yields nothing.
    #[test]
    fn inner_and_plain_block_comments_stay_quiet() {
        let line = prose_line();
        let inner_attr = format!("#![doc = \" {line} {line} {line} {line}\"]\n");
        let source = format!(
            "{inner_attr}/*! {line} {line} {line} {line} */\n/* {line} {line} {line} {line} */\nfn f() {{}}\n"
        );

        assert!(checks(&source).is_empty());
    }

    // ── Attribute docs ──

    /// Consecutive `#[doc = "..."]` attributes form one paragraph:
    /// over-budget joined prose errors with DOC007 at the first
    /// attribute line.
    #[test]
    fn doc_attribute_prose_errors_at_the_first_attribute_line() {
        let line = prose_line();
        let source = format!(
            "#[doc = \" {line}\"]\n#[doc = \" {line}\"]\n#[doc = \" {line}\"]\n#[doc = \" {line}\"]\nfn f() {{}}\n"
        );

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 1);
    }

    /// An 81-char attribute value warns with DOC008 on the measured
    /// length: the quote and its following space never count.
    #[test]
    fn long_doc_attribute_value_warns_doc008() {
        let long = "a".repeat(81);
        let source = format!("#[doc = \" {long}\"]\nfn f() {{}}\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(
            found[0].message.starts_with("line is 81 chars long."),
            "the leading space must not count: {}",
            found[0].message
        );
    }

    /// A 4-space-prefixed attribute value is prose, not indented code:
    /// the one-space strip leaves a 3-space lead, so an over-long value
    /// still fires DOC008.
    #[test]
    fn four_space_attribute_values_measure_as_prose_not_code() {
        let long = "d".repeat(81);
        let source = format!("#[doc = \"    {long}\"]\nfn f() {{}}\n");

        assert_eq!(codes(&checks(&source), CODE_LINE_LENGTH).len(), 1);
    }

    /// A trailing doc attribute (`#[doc = "..."] fn f() {}`) ends its
    /// item's doc: the next item's attribute never joins it, so four
    /// under-budget docs on consecutive rows stay quiet.
    #[test]
    fn trailing_doc_attributes_never_join_across_items() {
        let line = prose_line();
        let source = (0..4)
            .map(|i| format!("#[doc = \" {line}\"] fn f{i}() {{}}\n"))
            .collect::<String>();

        assert!(
            line.chars().count() * 4 + 3 > 240,
            "joined, the four docs must pass the paragraph budget"
        );
        assert!(checks(&source).is_empty());
    }

    /// Same-row doc attributes of one item join: their prose measures
    /// as one paragraph.
    #[test]
    fn same_row_doc_attributes_join_into_one_paragraph() {
        let line = prose_line();
        let source = format!(
            "#[doc = \" {line}\"] #[doc = \" {line}\"] #[doc = \" {line}\"] #[doc = \" {line}\"]\nfn f() {{}}\n"
        );

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].severity, Severity::Error);
    }

    /// A value spanning literal newlines keeps each line's original
    /// file number: the second value line reports at its own line.
    #[test]
    fn multi_line_attribute_values_keep_their_line_numbers() {
        let long = "c".repeat(81);
        let source = format!("#[doc = \" first\n {long}\"]\nfn f() {{}}\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2, "the value's second line");
    }

    /// An over-long tail after an escape sequence still fires DOC008:
    /// the value's later string-content fragments measure too.
    #[test]
    fn over_long_tail_after_an_escape_warns_doc008() {
        let long = "z".repeat(81);
        let source = format!("#[doc = \" head\\n {long}\"]\nfn f() {{}}\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert!(
            found[0].message.starts_with("line is 81 chars long."),
            "the tail fragment must measure in full: {}",
            found[0].message
        );
    }

    /// Fragments around escape sequences join into one paragraph: four
    /// under-80 chunks separated by escapes overflow DOC007 only when
    /// every fragment measures.
    #[test]
    fn escape_split_fragments_join_into_one_paragraph() {
        let chunk = "f".repeat(60);
        let source = format!("#[doc = \" {chunk}\\t{chunk}\\t{chunk}\\t{chunk}\"]\nfn f() {{}}\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    /// List-form `#[doc(...)]`, non-doc attributes, and doc-shaped text
    /// inside string literals never measure.
    #[test]
    fn non_doc_attributes_and_strings_stay_quiet() {
        let line = prose_line();
        let doc_like = format!("#[doc = \" {line} {line} {line} {line}\"]\n");
        let source = format!(
            "#[doc(hidden)]\n\
             #[allow(dead_code)]\n\
             fn f() {{}}\n\
             fn g() {{\n    let s = \"\n{doc_like}\";\n    let _ = s;\n}}\n"
        );

        assert!(checks(&source).is_empty());
    }
}
