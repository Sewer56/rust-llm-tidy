//! TEXT002: line length limit over the plaintext analysis.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_LINE_LENGTH;
use crate::text::measurement::{Document, StrippedLine};
use crate::text::measurement::{is_decorative_border, is_link_reference_definition};
use crate::text::urls;

/// Maximum line length before TEXT002 fires.
const LINE_LIMIT: usize = 80;

/// TEXT002 diagnostics for `doc`: one Warning per line over the limit, in
/// source order.
///
/// Measurement per line kind:
///
/// - Code-block lines, decorative borders, table rows, and link reference
///   definitions are skipped.
/// - A URL that ends the line is excluded from the count; trailing
///   punctuation and markdown closers after it still count.
/// - Every other character counts: code spans, link labels, mid-line URLs,
///   and link targets included.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in &doc.lines {
        if line.in_code_block {
            continue;
        }
        let trimmed = line.text.trim();
        if trimmed.starts_with('|')
            || is_link_reference_definition(trimmed)
            || is_decorative_border(trimmed)
        {
            continue;
        }
        let len = measured_length(trimmed);
        if len > LINE_LIMIT {
            diags.push(line_length_diagnostic(line, len));
        }
    }
    diags
}

/// TEXT002 Warning for one over-limit line; `len` is its measured length.
fn line_length_diagnostic(line: &StrippedLine, len: usize) -> Diagnostic {
    let bullets = [
        format!("Wrap prose at word boundaries to {LINE_LIMIT} chars or fewer per line."),
        "Preserve paragraph and list structure; do not split code identifiers, code spans, or URLs."
            .to_string(),
        "A URL at the end of the line is excluded from the count; URLs mid-line count."
            .to_string(),
        "Code blocks, table rows, and link definitions are exempt.".to_string(),
        "Borders are ignored.".to_string(),
    ];
    Diagnostic {
        title: Some("long line".into()),
        severity: Severity::Warning,
        code: CODE_LINE_LENGTH,
        message: bulleted(
            &format!("line is {len} chars long."),
            "Long lines are harder to follow in narrow editors and side-by-side reviews.",
            &bullets,
        ),
        line: line.number,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

/// Char count of `trimmed` without its trailing URL, when one is present.
fn measured_length(trimmed: &str) -> usize {
    let total = trimmed.chars().count();
    match urls::trailing_range(trimmed) {
        Some(url) => total - trimmed[url].chars().count(),
        None => total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use indoc::formatdoc;
    use rstest::rstest;

    // ── TEXT002: line length ──

    // Over-limit stripped line -> TEXT002 Warning with a measurement summary
    // plus safe wrapping guidance. Indent and comment marker are not
    // measured.
    #[test]
    fn text_checks_should_preserve_structure_when_wrapping_long_line() {
        let text = "x".repeat(LINE_LIMIT + 1);
        let source = format!("\t/// {text}\n");

        let diags = run_text_checks(&source, "rs");

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].line, 1);
        let msg = &found[0].message;
        assert!(msg.starts_with(&format!("line is {} chars long.", LINE_LIMIT + 1)));
        assert!(msg.contains("\nWhy: Long lines are harder to follow in narrow editors and side-by-side reviews.\nSuggestions:\n  - "));
        assert!(msg.contains(&format!(
            "Wrap prose at word boundaries to {LINE_LIMIT} chars or fewer per line."
        )));
        assert!(msg.contains("Preserve paragraph and list structure"));
        assert!(msg.contains("do not split code identifiers, code spans, or URLs"));
        assert!(msg.contains(
            "A URL at the end of the line is excluded from the count; URLs mid-line count"
        ));
        assert!(msg.contains("Code blocks, table rows, and link definitions are exempt"));
    }

    // A line of exactly 80 chars passes.
    #[test]
    fn text_checks_silent_on_line_at_limit() {
        let source = format!("{}\n", "x".repeat(80));
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    #[test]
    fn text_checks_should_ignore_long_borders_when_standalone() {
        for marker in ["-", "=", "_", "*", "+", "/", "\\", ".", ":", "─", "═", "█"] {
            let border = marker.repeat(LINE_LIMIT + 1);
            let source = format!("// {border}\n// {border} Label\n");

            let diags = run_text_checks(&source, "rs");

            let found = codes(&diags, CODE_LINE_LENGTH);
            assert_eq!(found.len(), 1, "{marker}");
            assert_eq!(found[0].line, 2, "{marker}");
        }
    }

    // Code-block lines are exempt: fenced and indented blocks never warn,
    // including an over-long fence delimiter with an info string.
    #[test]
    fn text_checks_silent_on_code_block_lines() {
        let fenced = "y".repeat(90);
        let fence_line = format!("```{}", "i".repeat(80));
        let md = formatdoc! {"
            {fence_line}
            {fenced}
            ```

                {fenced}
        "};
        assert!(codes(&run_text_checks(&md, "md"), CODE_LINE_LENGTH).is_empty());
        let indented_rs = format!("///     {}\n", "y".repeat(90));
        assert!(codes(&run_text_checks(&indented_rs, "rs"), CODE_LINE_LENGTH).is_empty());
    }

    // The same span-bearing text stays silent inside a code block but warns
    // as prose, where its code span counts.
    #[test]
    fn text_checks_exempt_code_block_but_count_prose_spans() {
        let inner = format!("say `{}` out loud", "b".repeat(80));
        let source = formatdoc! {"
            ```
            {inner}
            ```

            {inner}
        "};
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 5);
    }

    // Table rows are never measured, in Markdown and doc comments alike.
    #[test]
    fn text_checks_silent_on_table_rows() {
        let row = format!("| {} |", "cell ".repeat(20));
        let md = formatdoc! {"
            | a | b |
            | --- | --- |
            {row}
        "};
        assert!(codes(&run_text_checks(&md, "md"), CODE_LINE_LENGTH).is_empty());
        let rs = format!("/// {row}\n");
        assert!(codes(&run_text_checks(&rs, "rs"), CODE_LINE_LENGTH).is_empty());
    }

    // Link reference definitions are skipped whole, long labels included.
    #[test]
    fn text_checks_silent_on_link_reference_definitions() {
        let source = format!("[{}]: ./a/very/long/relative/target.md\n", "l".repeat(90));
        let diags = run_text_checks(&source, "md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // A mid-line URL counts in full: the line warns with its whole length.
    #[test]
    fn text_checks_count_mid_line_urls() {
        let source = format!(
            "see {} at https://example.com/x for details\n",
            "a".repeat(60)
        );
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert!(found[0].message.starts_with("line is 101 chars long."));
    }

    // A URL that ends the line is excluded, so a long line still passes:
    // bare, as a markdown link, as an autolink, or with a bracketed IPv6
    // host.
    #[rstest]
    #[case::bare(
        "See the reference at https://example.com/a/very/long/path/that/keeps/going/beyond/the/eighty/char/limit"
    )]
    #[case::link(
        "See [the reference](https://example.com/a/very/long/path/that/keeps/going/beyond/the/eighty/char/limit)."
    )]
    #[case::autolink(
        "See <https://example.com/a/very/long/path/that/keeps/going/beyond/the/eighty/char/limit>"
    )]
    #[case::ipv6_host(
        "See https://[::1]/a/very/long/path/that/keeps/going/beyond/the/eighty/char/limit/x"
    )]
    fn text_checks_exempt_trailing_url(#[case] source: &str) {
        assert!(
            source.chars().count() > LINE_LIMIT,
            "fixture must exceed the limit"
        );
        let diags = run_text_checks(&format!("{source}\n"), "md");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // Prose glued to a closed markdown URL is not trailing: it counts in
    // full and warns.
    #[rstest]
    #[case::autolink("See <https://a.test>")]
    #[case::link("See [docs](https://a.test)")]
    #[case::code_span("See `https://a.test`")]
    fn text_checks_count_prose_after_closed_url(#[case] prefix: &str) {
        let prose = "x".repeat(LINE_LIMIT + 1);
        let source = format!("{prefix}{prose}\n");
        let diags = run_text_checks(&source, "md");

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        let len = prefix.chars().count() + prose.chars().count();
        assert!(
            found[0]
                .message
                .starts_with(&format!("line is {len} chars long.")),
            "{prefix}: {}",
            found[0].message
        );
    }

    // A scheme with an empty destination is not a URL: the line counts in
    // full and warns.
    #[rstest]
    #[case::mailto("mailto:", 83)]
    #[case::reloaded_2("r2:", 82)]
    fn text_checks_count_empty_destination_scheme(#[case] tail: &str, #[case] len: usize) {
        // `len` counts the prose, the separating space, and the scheme.
        let prose = "x".repeat(len - tail.chars().count() - 1);
        let source = format!("{prose} {tail}\n");
        let diags = run_text_checks(&source, "md");

        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1, "{tail}");
        assert!(
            found[0]
                .message
                .starts_with(&format!("line is {len} chars long.")),
            "{tail}: {}",
            found[0].message
        );
    }

    // The trailing URL is excluded from the count, so the limit applies to
    // the prose before it. The separating space is prose, not URL.
    #[rstest]
    #[case::at_limit(79, None)]
    #[case::over_limit(80, Some(LINE_LIMIT + 1))]
    fn text_checks_bound_trailing_url_exemption_at_the_limit(
        #[case] prose: usize,
        #[case] expected: Option<usize>,
    ) {
        let source = format!("{} https://example.com/x\n", "x".repeat(prose));
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);

        match expected {
            Some(len) => {
                assert_eq!(found.len(), 1);
                assert!(
                    found[0]
                        .message
                        .starts_with(&format!("line is {len} chars long.")),
                    "the reported length excludes the URL: {}",
                    found[0].message
                );
            }
            None => assert!(found.is_empty()),
        }
    }

    // The warning reports the full line length, spans included.
    #[test]
    fn text_checks_reports_full_length_with_spans() {
        let source = format!("{} `{}`\n", "a".repeat(82), "b".repeat(20));
        let diags = run_text_checks(&source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.starts_with("line is 105 chars long."));
    }

    // Badge lines count in full: the URL is mid-line, so its image target
    // and the trailing reference label both measure.
    #[test]
    fn text_checks_count_badge_line_targets() {
        let source = "[![Crates.io](https://img.shields.io/badge/rust_llm_tidy-v0.1.0-orange.svg)][Crates.io CLI]\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.starts_with("line is 91 chars long."));
    }
}
