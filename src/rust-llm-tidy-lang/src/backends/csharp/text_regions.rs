//! The C# doc-region producer: `///` comment runs as XML-doc regions for
//! the TEXT001/TEXT002 text checks.
//!
//! One depth-first walk collects every `///` comment node of the parse
//! tree; consecutive comment rows group into one [`DocRegion`] per doc
//! run, and each line keeps its original 1-based file line number.
//!
//! String literals and code lines are never comment nodes, so their
//! content is never measured.
//!
//! [`DocRegion`]: rust_llm_tidy_lint::check::DocRegion

use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_lint::check::{Dialect, DocRegion, RegionLine, run_region_checks};
use rust_llm_tidy_model::parse::ParseResult;

/// Runs the TEXT001/TEXT002 text checks over `parsed`'s `///` doc runs,
/// measured with the XML doc dialect.
///
/// The findings carry original 1-based file lines and ride the same lint
/// codes and severities the markdown-family checks use.
pub(super) fn text_checks(parsed: &ParseResult) -> Vec<Diagnostic> {
    run_region_checks(doc_regions(parsed))
}

/// The file's `///` doc runs as XML-doc regions, in source order.
fn doc_regions(parsed: &ParseResult) -> Vec<DocRegion> {
    let source = parsed.source.as_str();
    let mut comments = Vec::new();
    collect_doc_comments(parsed.syntax_tree().root_node(), source, &mut comments);

    let mut regions: Vec<DocRegion> = Vec::new();
    for (row, start, end) in comments {
        // Strip the marker, at most one following space, and a trailing
        // CR of CRLF endings, mirroring the legacy line-marker producer.
        let text = &source[start..end];
        let text = text.strip_prefix("///").unwrap_or(text);
        let text = text.strip_prefix(' ').unwrap_or(text);
        let text = text.strip_suffix('\r').unwrap_or(text);
        let line = RegionLine {
            number: row + 1,
            text: text.to_string(),
            indented: false,
        };
        // A run continues while comment rows stay on adjacent (or equal)
        // rows; any gap of a non-comment line ends it. A later comment
        // can never rejoin the run: rows arrive in source order.
        let continues = regions
            .last()
            .is_some_and(|region| row <= region.lines.last().expect("run holds a line").number);
        if continues {
            regions.last_mut().expect("open region").lines.push(line);
        } else {
            regions.push(DocRegion {
                dialect: Dialect::XmlDoc,
                lines: vec![line],
            });
        }
    }
    regions
}

/// Collects `(row, start_byte, end_byte)` of every `///` comment node in
/// document order, walking the subtree depth-first on one reused cursor.
/// A fourth slash keeps the node an ordinary comment, so `////` runs
/// never join a doc region.
fn collect_doc_comments(
    root: tree_sitter::Node<'_>,
    source: &str,
    out: &mut Vec<(usize, usize, usize)>,
) {
    let mut cursor = root.walk();
    'walk: loop {
        let node = cursor.node();
        if node.kind() == "comment"
            && source.get(node.start_byte()..node.start_byte() + 3) == Some("///")
            && source.as_bytes().get(node.start_byte() + 3) != Some(&b'/')
        {
            out.push((
                node.start_position().row,
                node.start_byte(),
                node.end_byte(),
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_lint::Severity;
    use rust_llm_tidy_lint::check::{CODE_LINE_LENGTH, CODE_PARAGRAPH_SIZE};

    /// Parses `source` as C# and runs its text checks.
    fn checks(source: &str) -> Vec<Diagnostic> {
        text_checks(&super::super::parse::parse(source).unwrap())
    }

    /// The diagnostics carrying `code`.
    fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    // ── Quiet probes ──

    /// Idiomatic XML docs measure quietly: every text node is short, so no
    /// paragraph or line crosses its budget.
    #[test]
    fn idiomatic_xml_doc_yields_zero_findings() {
        let source = "\
/// <summary>Loads a value for the key.</summary>
/// <param name=\"key\">The key to look up.</param>
/// <exception cref=\"System.InvalidOperationException\">
/// Thrown when the key is empty.
/// </exception>
/// <returns>The loaded value.</returns>
public string Load(string key) { return key; }
";

        assert!(checks(source).is_empty());
    }

    /// The marker-scanner false-positive class: a doc run whose
    /// marker-stripped lines join far past the paragraph budget stays
    /// quiet because only text-node inner text is measured.
    #[test]
    fn long_attribute_values_stay_quiet() {
        let doc_lines = [
            "<summary>Saves the value under the key in the store.</summary>",
            "<param name=\"key\">The key.</param>",
            "<param name=\"value\">The value.</param>",
            "<param name=\"store\">The target store for the save operation.</param>",
            "<exception cref=\"System.Collections.Generic.KeyNotFoundException\">",
            "Missing key.",
            "</exception>",
            "<exception cref=\"System.ArgumentNullException\">",
            "Null key.",
            "</exception>",
            "<exception cref=\"System.ArgumentException\">",
            "Empty key or value.",
            "</exception>",
            "<seealso cref=\"System.Collections.Generic.Dictionary{TKey,TValue}.Add\"/>",
        ];
        let stripped = doc_lines.join(" ");
        assert!(
            stripped.chars().count() > 480,
            "probe must join past the 240 budget: {}",
            stripped.chars().count()
        );
        let source = format!(
            "/// {}\npublic void Save(string key, string value, string store) {{ }}\n",
            doc_lines.join("\n/// ")
        );

        assert!(checks(&source).is_empty());
    }

    /// `<code>` and `<example>` subtrees are exempt like code fences,
    /// including code-looking `<`, trailing comments, and long lines.
    #[test]
    fn code_and_example_subtrees_stay_quiet() {
        let source = "\
/// <summary>Runs the sample.</summary>
/// <example>
/// <code>
/// var loader = new Loader();
/// var value = loader.Load(\"key\"); // trailing note in code
/// if (value.Length < 3) { return; }
/// </code>
/// </example>
public void Sample() { }
";

        assert!(checks(source).is_empty());
    }

    /// `////` rulers are ordinary comments, not XML docs: an over-80
    /// ruler adjacent to a real doc run produces no warning.
    #[test]
    fn four_slash_rulers_stay_quiet() {
        let ruler = "-".repeat(85);
        let source = format!(
            "//// {ruler}\n/// <summary>Loads the value.</summary>\npublic int Load() => 1;\n"
        );

        assert!(checks(&source).is_empty());
    }

    /// Verbatim string content is never a comment node: `///`-looking and
    /// over-budget lines inside it stay unmeasured.
    #[test]
    fn verbatim_string_content_stays_quiet() {
        let source = "\
/// <summary>Returns the template.</summary>
public string Template()
{
    return @\"First line.
/// A doc-looking line inside the verbatim string with plenty of words to
/// stretch any paragraph budget far past two hundred forty characters.
Last line.\";
}
";

        assert!(checks(source).is_empty());
    }

    /// Prose never joins across tags: two texts that overflow the budget
    /// when joined stay silent inside their own tags.
    #[test]
    fn paragraphs_never_join_across_tags() {
        let chunk = "filler text ".repeat(4);
        let chunk = chunk.trim();
        let doc_lines = [
            "<summary>",
            chunk,
            chunk,
            chunk,
            chunk,
            "</summary>",
            "<param name=\"x\">",
            chunk,
            chunk,
            chunk,
            chunk,
            "</param>",
        ];
        let stripped = doc_lines.join(" ");
        assert!(stripped.chars().count() > 240);
        let source = format!(
            "/// {}\npublic int Build(int x) {{ return x; }}\n",
            doc_lines.join("\n/// ")
        );

        assert!(checks(&source).is_empty());
    }

    /// A code gap between two `///` runs ends each run: the prose
    /// paragraphs never join across the member between them.
    #[test]
    fn code_gaps_end_doc_runs() {
        let line = "word ".repeat(14);
        let run = format!("/// {line}\n/// {line}\n/// {line}\n");
        let source = format!("{run}public void A() {{ }}\n{run}public void B() {{ }}\n");

        assert!(
            line.trim().chars().count() * 6 > 240,
            "the joined runs must pass the paragraph budget"
        );
        assert!(checks(&source).is_empty());
    }

    // ── True positives ──

    /// Over-budget summary prose errors with TEXT001 at the paragraph's
    /// first line, keeping original file line numbers.
    #[test]
    fn oversized_summary_prose_errors_at_its_first_line() {
        let prose: String = (0..40)
            .map(|i| format!("filler word {i:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(prose.chars().count() > 240);
        let source = format!(
            "namespace N;\n\n/// <summary>\n/// {prose}\n/// </summary>\npublic void Op() {{ }}\n"
        );

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].line, 4, "the first prose line, not the tag line");
    }

    /// TEXT002 warns on the measured line only: a long attribute-heavy line
    /// with short inner text stays quiet, a long inner text warns.
    #[test]
    fn long_param_inner_text_warns_on_the_measured_line() {
        let inner = "d".repeat(81);
        let source = format!(
            "/// <param name=\"aVeryLongAttributeNamePushingTheRawLinePastBudgets\">ok</param>\n/// <param name=\"x\">{inner}</param>\npublic void Op(int x) {{ }}\n"
        );

        let diags = checks(&source);
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1, "only the long inner text warns");
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].severity, Severity::Warning);
    }

    /// The producer covers every doc run in the file, including enum
    /// member docs no declaration walk reaches.
    #[test]
    fn enum_member_docs_are_measured() {
        let prose: String = (0..40)
            .map(|i| format!("filler word {i:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        let source = format!(
            "/// <summary>Kind.</summary>\npublic enum Kind\n{{\n    /// <summary>{prose}</summary>\n    First,\n}}\n"
        );

        let diags = checks(&source);
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 4, "the enum member's doc line");
    }
}
