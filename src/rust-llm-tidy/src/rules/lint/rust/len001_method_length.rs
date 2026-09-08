//! `LEN001` - oversized function or method.
//!
//! [`check`] measures every function item in the retained tree-sitter
//! tree: free functions, methods in `impl` blocks, test functions, and
//! inner functions.
//!
//! Each is measured on its own braces, and the same lines also count
//! toward any enclosing function.
//!
//! A measured line is a physical line between the body braces that
//! holds code. Blank lines and comment-only lines never count, `//` and
//! `/* */` alike.
//!
//! The signature never counts either: the measured range starts after
//! the opening brace. A brace sharing its line with the signature or
//! with trailing code contributes nothing.
//!
//! The threshold reaches this rule through `check_file` (the config seam
//! MOD001 uses): `LanguageBackend::lint` never sees config values.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_LEN001;
use crate::source::ParseResult;
use core::ops::Range;

/// One measured function: the bytes strictly between its body braces,
/// the line its `fn` item starts on, and its name.
struct FnBody<'a> {
    /// Source bytes strictly between the body braces.
    inner: Range<usize>,
    /// 1-based line of the function item itself; preceding attributes
    /// and doc comments sit on earlier lines and do not move it.
    line: usize,
    /// The function's identifier text, if the tree names it.
    name: Option<&'a str>,
}

/// Emit a hint when a function body exceeds its configured line budget.
///
/// Walks the retained tree once, gathering every function item and every
/// comment span. Each body's measured lines are then counted in one
/// pass over its own line range. Nothing re-parses.
///
/// Fires per function when its count strictly exceeds `max_lines`: a
/// body at exactly `max_lines` stays silent. All functions are measured
/// alike; test functions and methods get no exemption.
///
/// # Arguments
///
/// - `parsed` - the parsed source result whose retained tree is walked.
/// - `max_lines` - the resolved `method_length.max_lines` budget.
pub(crate) fn check(parsed: &ParseResult, max_lines: usize) -> Vec<Diagnostic> {
    let source = parsed.source.as_str();
    let mut comments = Vec::new();
    let mut functions = Vec::new();
    walk(
        parsed.syntax_tree().root_node(),
        source,
        &mut comments,
        &mut functions,
    );
    let mut diags = Vec::new();
    for function in functions {
        let measured = measured_lines(source, &function.inner, &comments);
        if measured > max_lines {
            diags.push(diagnostic(
                function.name,
                measured,
                max_lines,
                function.line,
            ));
        }
    }
    diags
}

/// Build the hint stating the measured facts and the split advice.
fn diagnostic(name: Option<&str>, measured: usize, max_lines: usize, line: usize) -> Diagnostic {
    let name = name.unwrap_or("<unnamed>");
    Diagnostic {
        severity: Severity::Hint,
        code: CODE_LEN001,
        message: indoc::formatdoc! {"
            fn `{name}` has {measured} body lines (blank and comment-only lines excluded),
            over the {max_lines}-line budget (method_length.max_lines).
            Why:
            - Long functions can make readers track too much control flow and local state.
            - Named, cohesive steps can help readers follow the flow without tracking every detail.
            Suggestions:
            - Consider extracting cohesive steps into functions named for what they do,
              so the outer function reads as an overview of the flow.
            - Keep closely related work together. Avoid new types, forwarding wrappers,
              or a wider public API solely to shorten the body.
            - Preserve behavior and performance. Avoid extra allocations, cloning, or
              repeated work; measure performance-sensitive changes.
            - Mark extracted functions as `#[inline]` if needed.
            - Inner functions still count toward the enclosing body.
            - Keep the body intact if splitting would make it harder to follow or slower."},
        line,
        item_kind: "fn".to_string(),
        item_name: Some(name.to_string()),
    }
}

/// Count `inner`'s measured lines: physical lines that hold code.
///
/// Blank and comment-only lines never count. `comments` holds every
/// comment span in the file in document order; `first` advances through
/// it monotonically, so each span is considered once per function.
fn measured_lines(source: &str, inner: &Range<usize>, comments: &[(usize, usize)]) -> usize {
    let mut count = 0;
    let mut first = comments.partition_point(|&(start, _)| start < inner.start);
    let mut offset = inner.start;
    for line in source[inner.start..inner.end].split('\n') {
        // The piece after a trailing newline is not a physical line.
        if offset == inner.end {
            break;
        }
        let line_end = offset + line.len();
        if line_has_code(source, offset, line_end, comments, &mut first) {
            count += 1;
        }
        offset = line_end + 1;
    }
    count
}

/// Depth-first cursor walk over `root`, gathering function bodies and
/// comment spans in document order.
fn walk<'a>(
    root: tree_sitter::Node<'a>,
    source: &'a str,
    comments: &mut Vec<(usize, usize)>,
    functions: &mut Vec<FnBody<'a>>,
) {
    let mut cursor = root.walk();
    'walk: loop {
        let node = cursor.node();
        match node.kind() {
            // A body-less form (a trait method signature) parses as
            // `function_signature_item`; only bodied functions measure.
            "function_item" => {
                if let Some(body) = node.child_by_field_name("body") {
                    functions.push(FnBody {
                        // Clamp so an EOF-truncated body no wider than its
                        // `{` cannot invert the range and panic on slicing.
                        inner: {
                            let start = body.start_byte() + 1;
                            start..body.end_byte().saturating_sub(1).max(start)
                        },
                        line: node.start_position().row + 1,
                        name: node
                            .child_by_field_name("name")
                            .and_then(|name| name.utf8_text(source.as_bytes()).ok()),
                    });
                }
            }
            "line_comment" | "block_comment" => {
                comments.push((node.start_byte(), node.end_byte()));
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

/// Whether the physical line `line_start..line_end` holds any
/// non-whitespace byte outside comment spans.
///
/// A block comment spanning several lines suppresses every line it
/// covers. `first` is the monotone cursor into `comments`.
fn line_has_code(
    source: &str,
    line_start: usize,
    line_end: usize,
    comments: &[(usize, usize)],
    first: &mut usize,
) -> bool {
    let bytes = source.as_bytes();
    let mut segment = line_start;
    while segment < line_end {
        // Skip comments that end at or before the remaining segment.
        while *first < comments.len() && comments[*first].1 <= segment {
            *first += 1;
        }
        let Some(&(comment_start, comment_end)) = comments.get(*first) else {
            return has_code(bytes, segment, line_end);
        };
        if comment_start >= line_end {
            return has_code(bytes, segment, line_end);
        }
        // Code ahead of the comment, when present, decides the line.
        if comment_start > segment && has_code(bytes, segment, comment_start) {
            return true;
        }
        segment = comment_end;
    }
    false
}

/// Whether `bytes[start..end]` holds any non-whitespace byte.
fn has_code(bytes: &[u8], start: usize, end: usize) -> bool {
    bytes[start..end].iter().any(|b| !b.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;
    use rstest::rstest;

    /// Parse `source` and measure it against `max_lines`.
    fn checks(source: &str, max_lines: usize) -> Vec<Diagnostic> {
        check(&parse_source(source).unwrap(), max_lines)
    }

    /// `fn sized()` whose body holds exactly `count` measured lines: one
    /// statement per line, no blanks or comments.
    fn sized_fn(count: usize) -> String {
        let body: String = (0..count)
            .map(|i| format!("    let _v{i} = {i};\n"))
            .collect();
        format!("fn sized() {{\n{body}}}\n")
    }

    // ── firing and the strict boundary ──

    // Over the budget: one hint naming the fn, its measured count,
    // the budget, and the split advice.
    #[test]
    fn check_should_emit_a_hint_when_measured_lines_exceed_the_threshold() {
        let diags = checks(&sized_fn(3), 2);

        assert_eq!(diags.len(), 1, "3 measured lines > budget 2");
        assert_eq!(diags[0].code, CODE_LEN001);
        assert_eq!(diags[0].severity, Severity::Hint);
        assert_eq!(diags[0].line, 1, "the fn item's own line");
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("sized"));
        assert!(
            diags[0].message.starts_with(
                "fn `sized` has 3 body lines (blank and comment-only lines excluded),\n\
                 over the 2-line budget (method_length.max_lines)."
            ),
            "the hint must state the measured count and the budget: {}",
            diags[0].message
        );
        assert!(
            diags[0].message.contains(
                "Consider extracting cohesive steps into functions named for what they do"
            ),
            "the advice must be actionable: {}",
            diags[0].message
        );
    }

    // At or below the budget: silent, including an empty body.
    #[rstest]
    #[case::empty_body(0, 1)]
    #[case::at_threshold(2, 2)]
    #[case::under_threshold(1, 500)]
    fn stays_silent_at_or_below_the_threshold(#[case] body_lines: usize, #[case] max_lines: usize) {
        assert!(
            checks(&sized_fn(body_lines), max_lines).is_empty(),
            "{body_lines} measured lines must not exceed the budget of {max_lines}"
        );
    }

    // ── what a measured line is ──

    // Blank and comment-only lines (line and block comments alike) leave
    // the count; code with a trailing comment still counts once.
    #[test]
    fn excludes_blank_and_comment_only_lines_from_the_count() {
        let source = "\
fn sized() {
    let _a = 1; // trailing comment

    // comment-only line
    let _b = 2;

    /* a block comment
       spanning two lines */
    let _c = 3;
}
";

        assert!(
            checks(source, 3).is_empty(),
            "3 code lines must not exceed the budget of 3"
        );
        let diags = checks(source, 2);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.starts_with("fn `sized` has 3 body lines"),
            "only the code lines count: {}",
            diags[0].message
        );
    }

    // A multi-line signature and brace-only lines never count.
    #[test]
    fn never_counts_the_signature_line_or_brace_only_lines() {
        let source = "\
fn sized(
    input: u32,
)
{
    let _a = 1;
    let _b = 2;
}
";

        assert!(checks(source, 2).is_empty());
        let diags = checks(source, 1);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.starts_with("fn `sized` has 2 body lines"),
            "only the two statements count: {}",
            diags[0].message
        );
    }

    // Code after the closing brace on its own line belongs to that
    // trailing item, not to the measured body.
    #[test]
    fn ignores_trailing_code_on_the_closing_brace_line() {
        let source = "\
fn sized() {
    let _a = 1;
    let _b = 2;
} fn following() { let _c = 3; }
";

        assert!(
            checks(source, 2).is_empty(),
            "sized measures 2 and following 1; both stay within 2"
        );
        let diags = checks(source, 1);
        assert_eq!(diags.len(), 1, "only sized crosses the budget of 1");
        assert_eq!(diags[0].item_name.as_deref(), Some("sized"));
        assert!(
            diags[0].message.starts_with("fn `sized` has 2 body lines"),
            "the trailing item must not count toward sized: {}",
            diags[0].message
        );
    }

    // CRLF line endings measure exactly like LF ones.
    #[test]
    fn handles_crlf_line_endings() {
        let source =
            "fn sized() {\r\n    let _a = 1;\r\n    let _b = 2;\r\n    let _c = 3;\r\n}\r\n";

        let diags = checks(source, 2);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.starts_with("fn `sized` has 3 body lines"),
            "carriage returns are line dressing: {}",
            diags[0].message
        );
    }

    // ── every function item is measured ──

    // A method in an impl block measures like a free function and
    // reports at its own `fn` line.
    #[test]
    fn measures_methods_in_impl_blocks() {
        let source = format!(
            "struct T;\nimpl T {{\n    fn sized() {{\n{}}} \n}}\n",
            "        let _v = 1;\n".repeat(3)
        );

        let diags = checks(&source, 2);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3, "the method's `fn` line");
        assert_eq!(diags[0].item_kind, "fn");
        assert_eq!(diags[0].item_name.as_deref(), Some("sized"));
        assert!(
            diags[0].message.starts_with("fn `sized` has 3 body lines"),
            "{}",
            diags[0].message
        );
    }

    // A `#[test]` function gets no exemption.
    #[test]
    fn measures_test_functions_like_any_other() {
        let source = format!(
            "#[test]\nfn sized_behavior() {{\n{}}}\n",
            "    let _v = 1;\n".repeat(3)
        );

        let diags = checks(&source, 2);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2, "the `fn` line, not the attribute");
        assert_eq!(diags[0].item_name.as_deref(), Some("sized_behavior"));
    }

    // An inner function measures on its own, while its lines also count
    // toward the enclosing function.
    #[test]
    fn measures_inner_functions_and_their_enclosing_function() {
        let source = "\
fn outer() {
    let _a = 1;
    fn inner() {
        let _b = 2;
        let _c = 3;
    }
    let _d = 4;
}
";
        // outer measures 6: two own statements, four inner-fn lines.
        // inner measures 2.

        let diags = checks(source, 5);
        assert_eq!(diags.len(), 1, "only outer crosses 5");
        assert_eq!(diags[0].item_name.as_deref(), Some("outer"));
        assert!(
            diags[0].message.starts_with("fn `outer` has 6 body lines"),
            "the inner fn's lines count toward outer: {}",
            diags[0].message
        );

        let diags = checks(source, 1);
        assert_eq!(diags.len(), 2, "both cross 1");
        assert_eq!(
            (diags[0].item_name.as_deref(), diags[1].item_name.as_deref()),
            (Some("outer"), Some("inner")),
            "document order: the enclosing fn first"
        );
    }

    // One finding per oversized function, in document order.
    #[test]
    fn reports_each_oversized_function_in_document_order() {
        let source = format!("{}{}", sized_fn(3), sized_fn(3).replace("sized", "later"));

        let diags = checks(&source, 2);

        assert_eq!(
            diags
                .iter()
                .map(|d| (d.line, d.item_name.as_deref()))
                .collect::<Vec<_>>(),
            vec![(1, Some("sized")), (6, Some("later"))]
        );
    }

    // An EOF-truncated body (`{` with no closing brace) must stay silent
    // instead of panicking on a malformed body span.
    #[rstest]
    #[case::bare_open_brace("fn sized() {")]
    #[case::statements_then_eof("fn sized() {\n    let _a = 1;\n    let _b = 2;")]
    fn skips_eof_truncated_bodies(#[case] source: &str) {
        assert!(
            checks(source, 0).is_empty(),
            "a malformed body span must never measure or panic"
        );
    }
}
