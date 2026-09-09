//! Search original text in whole-file or comment-bounded regions.

use super::{SymbolObservations, observation};
use crate::config::{CompiledSymbolRule, RegexComments, SymbolMatcher};
use crate::languages::backend_for;
use crate::source::ParseResult;
use core::ops::Range;
use std::collections::BTreeMap;

/// Collect text hints independently of parsed symbol occurrences.
/// Uncertain comment recognition skips only rules requiring comment boundaries.
pub(crate) fn check(
    source: &str,
    ext: &str,
    parsed: Option<&ParseResult>,
    rules: &[CompiledSymbolRule],
) -> SymbolObservations {
    let applicable: Vec<_> = rules
        .iter()
        .filter(|rule| rule.is_text_regex() && rule.applies_to_extension(ext))
        .collect();
    let mut result = SymbolObservations::default();
    if applicable.is_empty() {
        return result;
    }

    let needs_comments = applicable
        .iter()
        .any(|rule| rule.comments != RegexComments::Include);
    let comments = needs_comments
        .then(|| comment_spans(source, ext, parsed))
        .flatten();
    if needs_comments && comments.is_none() {
        result.warnings.push(format!(
            "text regex rules with comments: exclude or only skipped for .{ext}: comments cannot be reliably identified; use comments: include to search all text"
        ));
    }
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
    let mut matches = BTreeMap::new();

    for rule in applicable {
        let SymbolMatcher::Regex(regex) = &rule.matcher else {
            unreachable!()
        };
        let mut search = |range: Range<usize>| {
            for found in regex.find_iter(&source[range.clone()]) {
                let start = range.start + found.start();
                matches.entry(start).or_insert_with(|| {
                    let line = starts.partition_point(|offset| *offset <= start);
                    observation(rule, found.as_str(), "text", line..=line)
                });
            }
        };

        match rule.comments {
            RegexComments::Include => search(0..source.len()),
            RegexComments::Only => {
                if let Some(comments) = &comments {
                    for range in comments {
                        search(range.clone());
                    }
                }
            }
            RegexComments::Exclude => {
                if let Some(comments) = &comments {
                    let mut start = 0;
                    for range in comments {
                        if start < range.start {
                            search(start..range.start);
                        }
                        start = range.end;
                    }
                    if start < source.len() || comments.is_empty() {
                        search(start..source.len());
                    }
                }
            }
        }
    }
    result.hints.extend(matches.into_values());
    result
}

/// Prefer retained trees, otherwise parse only when a comment selector needs it.
/// Docstrings and documentation attributes remain strings, not comments.
fn comment_spans(
    source: &str,
    ext: &str,
    parsed: Option<&ParseResult>,
) -> Option<Vec<Range<usize>>> {
    let owned;
    let parsed = if let Some(parsed) = parsed {
        parsed
    } else if let Some(backend) = backend_for(ext) {
        owned = backend.parse(source).ok()?;
        &owned
    } else {
        return crate::text::comments::comment_spans(source, ext);
    };
    let root = parsed.syntax_tree().root_node();
    if root.has_error() {
        return None;
    }

    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if matches!(node.kind(), "comment" | "line_comment" | "block_comment") {
            ranges.push(node.byte_range());
        } else if cursor.goto_first_child() {
            continue;
        }

        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Some(ranges);
            }
        }
    }
}
