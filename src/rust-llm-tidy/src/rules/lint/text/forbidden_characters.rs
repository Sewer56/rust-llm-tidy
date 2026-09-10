//! TEXT009 rejects scoped characters in docs and comment prose without edits.

use crate::config::ForbiddenCharacterRule;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::registry::CODE_FORBIDDEN_CHARACTERS;
use crate::source::ParseResult;
use crate::text::forbidden_character_regions::parsed_regions;
use crate::text::measurement::{Document, is_link_reference_definition, measure};
use std::collections::{HashMap, HashSet};

/// Check both origin categories from a retained parse with one resolved policy.
pub(crate) fn parsed_diagnostics(
    parsed: &ParseResult,
    ext: &str,
    rules: &[ForbiddenCharacterRule],
) -> Vec<Diagnostic> {
    let (docs, comments) = parsed_regions(parsed, ext);
    let mut diagnostics = scoped_diagnostics(&measure(docs), rules, true);
    diagnostics.extend(scoped_diagnostics(&measure(comments), rules, false));
    diagnostics.sort_by_key(|diagnostic| diagnostic.line);
    diagnostics
}

/// Check measured prose in source order, skipping code and link destinations.
pub(crate) fn diagnostics(doc: &Document, rules: &[ForbiddenCharacterRule]) -> Vec<Diagnostic> {
    scoped_diagnostics(doc, rules, true)
}

/// Check one origin category using only entries enabled for that category.
pub(crate) fn scoped_diagnostics(
    doc: &Document,
    rules: &[ForbiddenCharacterRule],
    docs: bool,
) -> Vec<Diagnostic> {
    let lookup: HashMap<_, _> = rules
        .iter()
        .filter(|rule| {
            if docs {
                rule.scope.docs
            } else {
                rule.scope.comments
            }
        })
        .flat_map(|rule| {
            rule.characters
                .iter()
                .map(move |&character| (character, rule))
        })
        .collect();
    if lookup.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let openers = matched_openers(doc);
    let mut code = 0;
    let mut destination_depth = 0usize;
    let mut previous_line = 0;
    for line in &doc.lines {
        if line.number != previous_line + 1 || line.in_code_block || line.text.trim().is_empty() {
            code = 0;
            destination_depth = 0;
        }
        previous_line = line.number;
        if line.in_code_block || is_link_reference_definition(line.text.trim_start()) {
            continue;
        }

        let mut chars = line.text.char_indices().peekable();
        let mut previous = '\0';
        let mut escaped = false;
        let mut angle = false;
        while let Some((offset, character)) = chars.next() {
            if (code != 0 || !escaped) && character == '`' && destination_depth == 0 && !angle {
                let mut run = 1;
                while chars.next_if(|&(_, ch)| ch == '`').is_some() {
                    run += 1;
                }
                if code == 0 && openers.contains(&(line.number, offset)) {
                    code = run;
                } else if code == run {
                    code = 0;
                }
                previous = '`';
                escaped = false;
                continue;
            }

            if code == 0 {
                if !escaped {
                    if character == '(' && (previous == ']' || destination_depth > 0) {
                        destination_depth += 1;
                    } else if character == ')' && destination_depth > 0 {
                        destination_depth -= 1;
                        previous = character;
                        continue;
                    } else if character == '<' && is_autolink(&line.text[offset + 1..]) {
                        angle = true;
                    } else if character == '>' && angle {
                        angle = false;
                        previous = character;
                        continue;
                    }
                }
                if destination_depth == 0
                    && !angle
                    && let Some(rule) = lookup.get(&character)
                {
                    findings.push(Diagnostic {
                        severity: Severity::Error,
                        code: CODE_FORBIDDEN_CHARACTERS,
                        title: Some(rule.title.clone()),
                        message: format!(
                            "forbidden character {character:?} (U+{:04X}).\n{}",
                            character as u32, rule.message
                        ),
                        line: line.number,
                        item_kind: "file".into(),
                        item_name: None,
                    });
                }
            }

            escaped = !escaped && character == '\\';
            previous = character;
        }
    }
    findings
}

/// Recognize complete URI/email autolinks, not comparison punctuation.
fn is_autolink(rest: &str) -> bool {
    let Some(end) = rest.find(['<', '>']) else {
        return false;
    };
    if rest.as_bytes()[end] != b'>' {
        return false;
    }
    let target = &rest[..end];
    !target.chars().any(char::is_whitespace)
        && (target.contains("://") || target.starts_with("mailto:") || target.contains('@'))
}

/// Record backtick runs with a matching closer before a paragraph boundary.
/// A reverse pass avoids repeated suffix scans for unmatched delimiters.
fn matched_openers(doc: &Document) -> HashSet<(usize, usize)> {
    let mut openers = HashSet::new();
    let mut lengths = HashSet::new();
    let mut next_line = None;
    for line in doc.lines.iter().rev() {
        if next_line.is_some_and(|number| number != line.number + 1)
            || line.in_code_block
            || line.text.trim().is_empty()
        {
            lengths.clear();
        }
        next_line = Some(line.number);
        if line.in_code_block {
            continue;
        }

        let mut runs = Vec::new();
        let mut chars = line.text.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            if ch != '`' {
                continue;
            }
            let mut length = 1;
            while chars.next_if(|&(_, ch)| ch == '`').is_some() {
                length += 1;
            }
            runs.push((offset, length));
        }

        for (offset, length) in runs.into_iter().rev() {
            if !lengths.insert(length) {
                openers.insert((line.number, offset));
            }
        }
    }
    openers
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Prose is checked while examples and destinations remain untouched.
    #[rstest]
    #[case::prose("Read\u{2014}this", 1)]
    #[case::heading("# Read\u{2014}this", 1)]
    #[case::table("| Read\u{2014}this |", 1)]
    #[case::inline("`Read\u{2014}this`", 0)]
    #[case::fence("```text\nRead\u{2014}this\n```", 0)]
    #[case::destination("[Read](https://example.test/a\u{2014}b)", 0)]
    #[case::label("[Read\u{2014}this](https://example.test)", 1)]
    #[case::unicode("日本語 café", 0)]
    #[case::comparison("Use x < y \u{2014} then stop.", 1)]
    #[case::autolink("<https://example.test/a\u{2014}b>", 0)]
    #[case::unmatched("A literal `\n\nRead\u{2014}this", 1)]
    #[case::code_backslash("`a\\` \u{2014}", 1)]
    #[case::multiple_backticks("``a`\u{2014}b``", 0)]
    #[case::multiline("`a\n\u{2014}b`", 0)]
    fn diagnostics_should_respect_prose_boundaries(#[case] source: &str, #[case] expected: usize) {
        let doc = crate::text::measurement::analyze(source, "md");

        let found = diagnostics(&doc, crate::config::forbidden_character_rule::defaults());

        assert_eq!(found.len(), expected);
    }

    /// Custom entries own their rendered title and guidance.
    #[test]
    fn diagnostics_should_render_custom_title_and_message() {
        let doc = crate::text::measurement::analyze("Clean\nBad!", "md");
        let rules = [ForbiddenCharacterRule {
            scope: Default::default(),
            characters: vec!['!'],
            title: "Be calm".into(),
            message: "Use a full stop.".into(),
        }];

        let found = diagnostics(&doc, &rules);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 2);
        assert_eq!(found[0].title(), "Be calm");
        assert_eq!(
            found[0].to_string(),
            "2: error[TEXT009]: Be calm: forbidden character '!' (U+0021).\nUse a full stop. (file)"
        );
    }
}
