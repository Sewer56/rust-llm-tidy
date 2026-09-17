//! Measure opener characters without separate-line references.

use super::super::urls;
use crate::text::measurement::Paragraph;

/// Counts retained source lines joined by one space without changing the paragraph.
pub(super) fn char_count(para: &Paragraph) -> usize {
    let mut size = 0;
    let mut retained = false;
    for (index, &(_, start)) in para.line_starts.iter().enumerate() {
        // Member offsets identify source lines; the byte before the next
        // member is the single joining space, not part of either line.
        let end = para
            .line_starts
            .get(index + 1)
            .map_or(para.text.len(), |&(_, next)| next - 1);
        let line = &para.text[start..end];
        if is_link_line(line) {
            continue;
        }

        size += usize::from(retained) + line.chars().count();
        retained = true;
    }
    size
}

/// Accepts a bare URL or autolink, optionally after one whitespace-delimited word.
fn is_link_line(line: &str) -> bool {
    let Some(url) = urls::trailing_range(line) else {
        return false;
    };

    let prefix = &line[..url.start];
    let prefix = if let Some(label) = prefix.strip_suffix('<') {
        // Only a closed autolink is a supported wrapper. Punctuation may
        // sit inside it, outside it, or both under the shared URL policy.
        if !line[url.end..].contains('>') {
            return false;
        }
        label
    } else {
        prefix
    };
    if !prefix.is_empty() && !prefix.ends_with(char::is_whitespace) {
        return false;
    }

    let mut words = prefix.split_whitespace();
    match words.next() {
        None => true,
        Some(word) => words.next().is_none() && urls::trailing_range(word).is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use crate::rules::registry::{
        CODE_HEADER_OPENER, CODE_LINE_LENGTH, CODE_PARAGRAPH_SIZE, CODE_SENTENCE_LENGTH,
    };
    use crate::text::measurement::analyze;
    use rstest::rstest;

    // Core measurement

    #[rstest]
    #[case::bare("https://example.test/{path}", true)]
    #[case::autolink("<https://example.test/{path}>", true)]
    #[case::label("See https://example.test/{path}", true)]
    #[case::label_autolink("See <https://example.test/{path}>", true)]
    #[case::label_colon("Reference: https://example.test/{path}", true)]
    #[case::whitespace("  See:\t  https://example.test/{path}  ", true)]
    #[case::two_words("See also https://example.test/{path}", false)]
    #[case::after_url("https://example.test/{path} explains the behavior.", false)]
    #[case::after_label_url("See https://example.test/{path} for details.", false)]
    #[case::two_urls("https://example.test/first https://example.test/{path}", false)]
    #[case::two_autolinks("<https://example.test/first> <https://example.test/{path}>", false)]
    #[case::inline_link("[Reference](https://example.test/{path})", false)]
    #[case::label_inline_link("See [Reference](https://example.test/{path})", false)]
    #[case::ordinary("Ordinary {path}", false)]
    #[case::unknown_scheme("gopher://example.test/{path}", false)]
    #[case::compound_scheme("git+https://example.test/{path}", false)]
    #[case::glued_label("See<https://example.test/{path}>", false)]
    #[case::unclosed_autolink("<https://example.test/{path}", false)]
    #[case::code_span("`https://example.test/{path}`", false)]
    fn char_count_should_exclude_only_eligible_lines(
        #[case] template: &str,
        #[case] excluded: bool,
    ) {
        // Arrange
        let line = template.replace("{path}", &"x".repeat(180));
        let source = format!("Lead\n{line}\nTail");
        let doc = analyze(&source, "md");
        let para = &doc.paragraphs[0];

        // Act
        let measured = char_count(para);

        // Assert
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(measured, if excluded { 9 } else { para.size });
    }

    #[rstest]
    #[case::before("{url}\nLead\nTail", 9)]
    #[case::between("Lead\n{url}\nTail", 9)]
    #[case::after("Lead\nTail\n{url}", 9)]
    #[case::multiple("{url}\nLead\nSee {url}\nTail\nReference: <{url}>", 9)]
    #[case::only_links("{url}\nSee <{url}>", 0)]
    #[case::unicode("é界\n{url}\n🦀", 4)]
    fn char_count_should_join_retained_source_lines(
        #[case] template: &str,
        #[case] expected: usize,
    ) {
        // Arrange
        let source = template.replace(
            "{url}",
            &format!("https://example.test/{}", "x".repeat(180)),
        );
        let doc = analyze(&source, "md");

        // Act
        let measured = char_count(&doc.paragraphs[0]);

        // Assert
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(measured, expected);
    }

    #[test]
    fn char_count_should_measure_retry_after_reproduction_as_108_chars() {
        // Arrange
        let source = "// Simulate a server response with a retry-after header.\n\
            // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Retry-After#dealing_with_scheduled_downtime\n\
            // The header can accept time in seconds, or a HTTP date.\n";
        let doc = analyze(source, "rs");

        // Act
        let measured = char_count(&doc.paragraphs[0]);
        let diags = run_text_checks(source, "rs");

        // Assert
        assert_eq!(doc.paragraphs[0].size, 220);
        assert_eq!(measured, 108);
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
    }

    // URL policy edges

    #[rstest]
    #[case::http("http://example.test")]
    #[case::https("HTTPS://example.test")]
    #[case::ftp("ftp://example.test")]
    #[case::ftps("ftps://example.test")]
    #[case::ssh("ssh://example.test")]
    #[case::git("git://example.test")]
    #[case::ws("ws://example.test")]
    #[case::wss("wss://example.test")]
    #[case::file("file:///tmp/reference")]
    #[case::mailto("mailto:team@example.test")]
    #[case::nxm("nxm://example.test/mods/1")]
    #[case::r2("r2:mods/1")]
    #[case::punctuation("https://example.test.,;:!?'\"")]
    #[case::autolink_punctuation("<https://example.test>.")]
    #[case::balanced_parens("https://example.test/x_(y)")]
    #[case::ipv6("https://[::1]/status")]
    fn is_link_line_should_reuse_recognized_url_policy(#[case] line: &str) {
        // Act
        let excluded = is_link_line(line);

        // Assert
        assert!(excluded);
    }

    #[rstest]
    #[case::empty_http("https://")]
    #[case::empty_mailto("mailto:")]
    #[case::empty_r2("r2:")]
    #[case::punctuation_destination("https://...")]
    fn is_link_line_should_retain_empty_destinations(#[case] line: &str) {
        // Act
        let excluded = is_link_line(line);

        // Assert
        assert!(!excluded);
    }

    // Preserved thresholds and text rules

    #[rstest]
    #[case::ascii_limit("x", 160, false)]
    #[case::ascii_over("x", 161, true)]
    #[case::unicode_limit("界", 160, false)]
    #[case::unicode_over("界", 161, true)]
    fn diagnostics_should_use_retained_character_count(
        #[case] character: &str,
        #[case] size: usize,
        #[case] warns: bool,
    ) {
        // Arrange
        let source = format!(
            "https://example.test/{}\n{}",
            "x".repeat(180),
            character.repeat(size)
        );

        // Act
        let diags = run_text_checks(&source, "md");

        // Assert
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), usize::from(warns));
        if warns {
            assert_eq!(found[0].line, 1);
            assert!(
                found[0]
                    .message
                    .starts_with("opener paragraph is 161 chars long;")
            );
            assert!(found[0].message.contains("Put links on a separate line."));
        }
    }

    #[rstest]
    #[case::first("{url}\n{prose}", Some(1))]
    #[case::after_heading("Lead\n\n# Heading\n{url}\n{prose}", Some(4))]
    #[case::non_opener("Lead\n\n{url}\n{prose}", None)]
    #[case::only_links_first("{url}\n\n{prose}", None)]
    #[case::reference_definition("[ref]: {url}\n{prose}", Some(2))]
    fn diagnostics_should_preserve_opener_selection_and_locations(
        #[case] template: &str,
        #[case] expected_line: Option<usize>,
    ) {
        // Arrange
        let source = template
            .replace("{url}", "https://example.test/reference")
            .replace("{prose}", &"x".repeat(161));

        // Act
        let diags = run_text_checks(&source, "md");

        // Assert
        let lines: Vec<_> = codes(&diags, CODE_HEADER_OPENER)
            .iter()
            .map(|d| d.line)
            .collect();
        assert_eq!(lines, expected_line.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn diagnostics_should_preserve_sentences_when_url_line_is_excluded() {
        // Arrange
        let source = format!(
            "One.\nhttps://example.test/{}.\nTwo. Three.",
            "x".repeat(180)
        );

        // Act
        let diags = run_text_checks(&source, "md");

        // Assert
        let found = codes(&diags, CODE_HEADER_OPENER);
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .starts_with("opener paragraph has 3 sentences;")
        );
        assert!(!found[0].message.contains("Put links on a separate line."));
    }

    #[test]
    fn diagnostics_should_preserve_other_rule_measurements() {
        // Arrange
        let source = format!(
            "{}\nmailto:{}",
            "word ".repeat(26).trim_end(),
            "x".repeat(210)
        );

        // Act
        let diags = run_text_checks(&source, "md");

        // Assert
        assert!(codes(&diags, CODE_HEADER_OPENER).is_empty());
        let paragraphs = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(paragraphs.len(), 1);
        assert!(
            paragraphs[0]
                .message
                .starts_with("paragraph is 347 chars long.")
        );
        let lines = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line, 1);
        assert!(lines[0].message.starts_with("line is 129 chars long."));
        let sentences = codes(&diags, CODE_SENTENCE_LENGTH);
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0].line, 1);
        assert!(
            sentences[0]
                .message
                .starts_with("sentence is 27 words long.")
        );
    }
}
