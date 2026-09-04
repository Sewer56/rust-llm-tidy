//! The Python doc-region producer: docstring and `#`-comment regions for
//! the TEXT001/TEXT002 text checks of `py` and `pyi` sources.
//!
//! [`text_checks`] walks one tree-sitter-python parse in document order
//! and emits two kinds of [`DocRegion`] for the lint crate's measuring
//! core:
//!
//! - a triple-quoted string that is the first statement of a module,
//!   class, or function becomes a [`Docstring`]-dialect region, with the
//!   quotes stripped and the docstring's common indentation removed;
//! - `#` comments become markdown-prose regions exactly as the comment
//!   lexicon measured them: contiguous standalone runs join one region,
//!   the marker run and one space strip, and a trailing comment is its
//!   own region.
//!
//! Every other triple-quoted string is string content, and code lines
//! are code: neither ever measures.
//!
//! # Fail-closed
//!
//! A parse tree carrying error nodes produces no findings: a mis-scoped
//! string in a broken tree would risk measuring string content as prose,
//! so invalid sources stay silent instead of guessed.
//!
//! [`DocRegion`]: rust_llm_tidy_lint::check::DocRegion
//! [`Docstring`]: rust_llm_tidy_lint::check::Dialect::Docstring

use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_lint::check::{Dialect, DocRegion, RegionLine, run_region_checks};
use rust_llm_tidy_model::parse::ParseResult;

/// Parses `source` with the pinned Python grammar into the shared item
/// model.
///
/// Python implements no AST ops, so the result carries zero items; the
/// parse exists for [text_checks]' doc-region walk, which reads the tree
/// and the source through it.
///
/// # Arguments
///
/// - `source`: the file's text to parse.
///
/// # Errors
///
/// Returns an error when the grammar cannot be constructed or
/// tree-sitter fails to produce a syntax tree.
///
/// [text_checks]: self::text_checks
pub fn parse(source: &str) -> anyhow::Result<ParseResult> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language()?)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter-python produced no tree"))?;
    Ok(ParseResult::new(
        Vec::new(),
        source.to_string(),
        tree,
        0,
        source.len(),
    ))
}

/// Runs the TEXT001/TEXT002 text checks over `parsed`'s docstrings and `#`
/// comments.
///
/// The findings carry original 1-based file lines and ride the same lint
/// codes and severities the other producers use. A parse tree carrying
/// error nodes produces no findings (see the module docs).
///
/// # Arguments
///
/// - `parsed`: the parse produced by [`parse`], over the same source.
pub fn text_checks(parsed: &ParseResult) -> Vec<Diagnostic> {
    if parsed.syntax_tree().root_node().has_error() {
        return Vec::new();
    }
    run_region_checks(doc_regions(parsed))
}

/// The tree-sitter-python grammar the backend parses with.
///
/// # Errors
///
/// Returns an error when the bundled grammar cannot convert into a
/// [`tree_sitter::Language`] (cannot happen with the pinned grammar
/// version).
pub(crate) fn language() -> anyhow::Result<tree_sitter::Language> {
    Ok(tree_sitter_python::LANGUAGE.into())
}

/// The file's docstring and comment regions, in source order.
fn doc_regions(parsed: &ParseResult) -> Vec<DocRegion> {
    let source = parsed.source.as_str();
    let root = parsed.syntax_tree().root_node();
    let mut regions = Vec::new();
    // The open standalone-comment run; every other region closes it.
    let mut run: Option<DocRegion> = None;
    walk(root, source, &mut regions, &mut run);
    // The module body is visited before its children, so its docstring
    // lands ahead of leading comments that precede it in the file; a
    // stable sort by first line restores source order.
    regions.sort_by_key(|region| region.lines[0].number);
    regions
}

/// Walks `node`'s subtree in document order, appending docstring and
/// comment regions to `regions`; `run` carries the open standalone
/// comment run.
fn walk(
    node: tree_sitter::Node<'_>,
    source: &str,
    regions: &mut Vec<DocRegion>,
    run: &mut Option<DocRegion>,
) {
    let mut cursor = node.walk();
    'walk: loop {
        let current = cursor.node();
        if current.kind() == "comment" {
            comment_region(current, source, regions, run);
        } else if is_docstring_body(current)
            && let Some(doc) = docstring_region(current, source)
        {
            close_run(run, regions);
            regions.push(doc);
        }
        if cursor.goto_first_child() {
            continue 'walk;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == node {
                close_run(run, regions);
                return;
            }
        }
    }
}

/// Measures one `#` comment node: standalone comments join the open run
/// on adjacent rows, a trailing comment (code before the marker) is its
/// own region, and the marker run plus one space strips.
///
/// The measurement matches the comment lexicon's `#`-family output.
fn comment_region(
    node: tree_sitter::Node<'_>,
    source: &str,
    regions: &mut Vec<DocRegion>,
    run: &mut Option<DocRegion>,
) {
    let row = node.start_position().row + 1;
    let line_start = source[..node.start_byte()].rfind('\n').map_or(0, |i| i + 1);
    let standalone = source[line_start..node.start_byte()].trim().is_empty();
    let raw = &source[node.start_byte()..node.end_byte()];
    let text = raw.trim_start_matches('#');
    let text = text.strip_prefix(' ').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    let line = RegionLine {
        number: row,
        text: text.to_string(),
        indented: text.starts_with('\t') || text.starts_with("    "),
    };
    if standalone {
        // A run continues while comment rows stay adjacent; any gap of a
        // non-comment line ends it.
        let continues = run
            .as_ref()
            .and_then(|region| region.lines.last())
            .is_some_and(|last| last.number + 1 == row);
        if continues {
            run.as_mut().expect("open run").lines.push(line);
        } else {
            close_run(run, regions);
            *run = Some(DocRegion {
                dialect: Dialect::Markdown,
                lines: vec![line],
            });
        }
    } else {
        // A trailing comment never joins a run: fragments on consecutive
        // lines cannot pool into one paragraph.
        close_run(run, regions);
        regions.push(DocRegion {
            dialect: Dialect::Markdown,
            lines: vec![line],
        });
    }
}

/// The docstring region of `body`, or `None` when its first statement is
/// not a triple-quoted string.
///
/// Comments are not statements, so leading comment rows never displace
/// the first statement.
///
/// The region's lines are the string's content between the quote
/// delimiters, numbered from the original file lines and dedented by the
/// docstring's common continuation indent.
fn docstring_region(body: tree_sitter::Node<'_>, source: &str) -> Option<DocRegion> {
    let mut body_cursor = body.walk();
    let first = body
        .named_children(&mut body_cursor)
        .find(|child| child.kind() != "comment")?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let mut stmt_cursor = first.walk();
    let string = first.named_children(&mut stmt_cursor).next()?;
    if string.kind() != "string" {
        return None;
    }
    // Triple-quoted strings only: the opening delimiter (with any `r`/
    // `f` prefix) ends in three quote characters.
    let count = string.child_count();
    if count < 2 {
        return None;
    }
    let open = string.child(0)?;
    let close = string.child(u32::try_from(count - 1).ok()?)?;
    if open.kind() != "string_start" || close.kind() != "string_end" {
        return None;
    }
    let open_text = &source[open.start_byte()..open.end_byte()];
    if !(open_text.ends_with("\"\"\"") || open_text.ends_with("'''")) {
        return None;
    }

    let content = &source[open.end_byte()..close.start_byte()];
    let raw_lines: Vec<&str> = content.split('\n').collect();
    // The docstring's continuation indent: the common leading whitespace
    // of the lines after the first, which sits at the body's own indent.
    // The first line starts right after the quotes and never carries it.
    let margin = raw_lines[1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);

    let first_row = open.end_position().row;
    let mut lines = Vec::with_capacity(raw_lines.len());
    for (i, raw) in raw_lines.iter().enumerate() {
        let mut text = raw.strip_suffix('\r').unwrap_or(raw);
        if i > 0 {
            let lead = text.len() - text.trim_start_matches([' ', '\t']).len();
            text = &text[lead.min(margin)..];
        }
        lines.push(RegionLine {
            number: first_row + i + 1,
            text: text.to_string(),
            indented: text.starts_with('\t') || text.starts_with("    "),
        });
    }
    Some(DocRegion {
        dialect: Dialect::Docstring,
        lines,
    })
}

/// Whether `node` is a body whose first statement may be its docstring:
/// the module root, or the block of a class or function definition. Other
/// blocks (`if`, `for`, `while`) never carry docstrings.
fn is_docstring_body(node: tree_sitter::Node<'_>) -> bool {
    match node.kind() {
        "module" => true,
        "block" => matches!(
            node.parent(),
            Some(parent) if matches!(parent.kind(), "class_definition" | "function_definition")
        ),
        _ => false,
    }
}

/// Flushes the open standalone-comment run into `regions`, if any.
fn close_run(run: &mut Option<DocRegion>, regions: &mut Vec<DocRegion>) {
    if let Some(region) = run.take() {
        regions.push(region);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_lint::Severity;
    use rust_llm_tidy_lint::check::{CODE_LINE_LENGTH, CODE_PARAGRAPH_SIZE};

    /// Parses `source` and runs its text checks.
    fn checks(source: &str) -> Vec<Diagnostic> {
        text_checks(&parse(source).unwrap())
    }

    /// The diagnostics carrying `code`.
    fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    /// Five prose lines whose joined size crosses the 240 budget while
    /// each line stays under 80.
    const PROSE: &[&str] = &[
        "filler words pad the paragraph past the two hundred forty limit",
        "filler words pad the paragraph past the two hundred forty limit",
        "filler words pad the paragraph past the two hundred forty limit",
        "filler words pad the paragraph past the two hundred forty limit",
        "filler words pad the paragraph past the two hundred forty limit",
    ];

    /// Wraps the prose lines in an indented triple-quoted block: the
    /// opening quotes on their own line, the closing quotes on the last
    /// prose line.
    fn docstring(indent: &str) -> String {
        let mut out = format!("{indent}\"\"\"\n");
        for (i, line) in PROSE.iter().enumerate() {
            out.push_str(indent);
            out.push_str(line);
            if i + 1 == PROSE.len() {
                out.push_str("\"\"\"");
            }
            out.push('\n');
        }
        out
    }

    // ── Docstring true positives ──

    // Module, class, and function docstrings measure with original file
    // lines, each paragraph erroring at its first prose line.
    //
    // The class and function cases carry the body indent that the
    // producer must dedent: unstripped, every line would count as
    // indented code and never measure.
    #[test]
    fn module_class_and_function_docstrings_measure() {
        let module = docstring("");
        let diags = checks(&module);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "the module docstring: {diags:?}");
        assert_eq!(found[0].line, 2, "the first prose line");
        assert_eq!(found[0].severity, Severity::Error);

        let class = format!("class A:\n{}", docstring("    "));
        let diags = checks(&class);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "the class docstring: {diags:?}");
        assert_eq!(found[0].line, 3, "the first dedented prose line");

        let function = format!("def f():\n{}", docstring("    "));
        let diags = checks(&function);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "the function docstring: {diags:?}");
        assert_eq!(found[0].line, 3, "the first dedented prose line");
    }

    // A single-line docstring measures: an over-80 line warns, and an
    // `r` prefix or `'''` quotes never shift the measured span.
    #[test]
    fn single_line_raw_and_single_quoted_docstrings_measure() {
        for (prefix, quotes) in [("", "\"\"\""), ("r", "\"\"\""), ("", "'''")] {
            let long = "x".repeat(81);
            let source = format!("def f():\n    {prefix}{quotes}{long}{quotes}\n");

            let diags = checks(&source);
            let found = codes(&diags, CODE_LINE_LENGTH);
            assert_eq!(found.len(), 1, "`{prefix}{quotes}`: the long line");
            assert_eq!(found[0].line, 2);
            assert_eq!(found[0].severity, Severity::Warning);
        }
    }

    // A `>>>` doctest example inside a docstring stays quiet, prose
    // around it measures.
    #[test]
    fn doctest_lines_are_exempt() {
        let example = "c".repeat(90);
        let source = format!(
            "\"\"\"Loads the value.\n\n>>> value = load(key)\n>>> process({example})\n{example}\n\"\"\"\n"
        );

        assert!(checks(&source).is_empty());
    }

    // ── Non-docstring strings stay quiet ──

    // A triple-quoted string that is not a first statement is string
    // content: assigned, second-statement, and block-local strings never
    // measure, while the real module docstring and comment run do.
    //
    // The in-string payloads are plain prose that crosses the paragraph
    // budget - `#`-led filler would read as headings and stay exempt if
    // measured - so a loosened first-statement or body gate fires and
    // fails the count.
    #[test]
    fn non_docstring_triple_quoted_strings_stay_quiet() {
        let payload = "filler words pad the paragraph past the two hundred forty limit";
        let triple: String = (0..5).map(|_| format!("{payload}\n")).collect();
        let comment: String = (0..5)
            .map(|_| "# filler words pad the paragraph past the two hundred forty limit\n")
            .collect();
        let source = format!(
            "\"\"\"Module doc.\"\"\"\n\
             s = \"\"\"\n{triple}\"\"\"\n\
             def f():\n    \
             x = 1\n    \"\"\"\n    {triple}    \"\"\"\n    \
             if x:\n        \"\"\"\n        {triple}        \"\"\"\n        \
             pass\n\
             # real\n{comment}"
        );

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(
            found.len(),
            1,
            "only the trailing comment paragraph, never string content:\n{diags:?}"
        );
        assert_eq!(
            found[0].line, 27,
            "the trailing comment run's first line (the `# real` lead joins it)"
        );
    }

    // ── Comment coverage ──

    // Standalone `#` comment runs measure as one paragraph per run, and
    // a trailing comment is its own region: fragments never pool - the
    // fragments join past the paragraph budget, so pooling would fire.
    #[test]
    fn comment_runs_measure_and_trailing_comments_isolate() {
        let fragment = "fragments of trailing comments that must never pool into one paragraph";
        let source = format!(
            "# {fragment}\n# {fragment}\nx = foo(a,  # {fragment}\n         b)  # {fragment}\n"
        );
        let diags = checks(&source);
        assert!(
            codes(&diags, CODE_PARAGRAPH_SIZE).is_empty(),
            "fragments must not pool: {diags:?}"
        );

        let comment: String = (0..5)
            .map(|_| "# filler words pad the paragraph past the two hundred forty limit\n")
            .collect();
        let source = format!("{comment}x = 1\n");
        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "the standalone run measures");
        assert_eq!(found[0].line, 1);
    }

    // A leading comment run and the module docstring both fire in file
    // order: the docstring never outruns comments above it.
    #[test]
    fn leading_comment_findings_precede_the_module_docstring() {
        let comment: String = (0..5)
            .map(|_| "# filler words pad the paragraph past the two hundred forty limit\n")
            .collect();
        let source = format!("{comment}{}", docstring(""));

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        let lines: Vec<usize> = found.iter().map(|d| d.line).collect();
        assert_eq!(
            lines,
            [1, 7],
            "the comment paragraph first, then the docstring"
        );
    }

    // A trailing comment on the definition line sorts before the
    // docstring: findings report in file-line order.
    #[test]
    fn def_line_comment_sorts_before_the_docstring() {
        let long = "x".repeat(81);
        let source = format!("def f():  # {long}\n    \"\"\"{long}\"\"\"\n    return 1\n");

        let diags = checks(&source);
        let found = codes(&diags, CODE_LINE_LENGTH);
        let lines: Vec<usize> = found.iter().map(|d| d.line).collect();
        assert_eq!(lines, [1, 2], "trailing comment first, docstring second");
    }

    // ── Fail-closed ──

    // A parse tree with error nodes produces no findings, even where a
    // real comment would measure - broken syntax and an unterminated
    // triple-quoted string alike.
    #[test]
    fn invalid_sources_produce_no_findings() {
        let comment: String = (0..5)
            .map(|_| "# filler words pad the paragraph past the two hundred forty limit\n")
            .collect();
        for probe in ["def broken(:\n    pass\n", "s = \"\"\"\n"] {
            let source = format!("{probe}{comment}");

            assert!(
                checks(&source).is_empty(),
                "invalid source must stay silent: {probe:?}"
            );
        }
    }

    // A docstring after a leading comment still measures: comments are
    // not statements, so they never displace the first statement.
    #[test]
    fn leading_comments_do_not_displace_the_module_docstring() {
        let source = format!("# lead\n{}", docstring(""));

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3, "the docstring's first prose line");
    }
}
