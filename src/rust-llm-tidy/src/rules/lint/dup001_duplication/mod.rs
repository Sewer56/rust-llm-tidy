//! Find repeated source sequences with a complete query in eligible lines.
//!
//! Minimum-size windows seed a per-file fingerprint table. The matcher checks
//! every fingerprint hit against normalized lines before grouping.
//!
//! Extended spans retain internal
//! blank and punctuation lines, but begin and end on meaningful lines. No syntax
//! or semantic equivalence is inferred.

use crate::config::DuplicationConfig;
use crate::input::changed_lines::ChangedLines;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::registry::CODE_DUPLICATION;
use ahash::AHashMap;
use core::fmt::Write;
use groups::{Group, select_sites};
use source::Lines;

mod groups;
mod source;
#[cfg(test)]
mod tests;

/// Analyze one source buffer without I/O or edits; empty authority does no
/// work. NUL-containing buffers are treated as binary and return none.
pub(crate) fn check(
    source: &str,
    eligible: &ChangedLines,
    config: DuplicationConfig,
) -> Vec<Diagnostic> {
    analyze(source, eligible, config, u64::MAX)
}

/// Keep collision injection private to matcher tests; production retains every bit.
fn analyze(
    source: &str,
    eligible: &ChangedLines,
    config: DuplicationConfig,
    fingerprint_mask: u64,
) -> Vec<Diagnostic> {
    if eligible.is_empty() || source.contains('\0') {
        return Vec::new();
    }
    let lines = Lines::new(source, eligible, config.exact_whitespace);
    let minimum = config.min_meaningful_lines;
    if minimum == 0 || config.min_occurrences < 2 || minimum > lines.meaningful.len() {
        return Vec::new();
    }

    let mut seeds: AHashMap<(u64, usize), Vec<Group>> = AHashMap::new();
    for window in lines.meaningful.windows(minimum) {
        let start = window[0];
        let end = window[minimum - 1] + 1;
        let key = (
            lines.fingerprint(start, end) & fingerprint_mask,
            end - start,
        );
        let bucket = seeds.entry(key).or_default();
        if let Some(group) = bucket
            .iter_mut()
            .find(|group| lines.equal(group.starts[0], start, end - start))
        {
            group.starts.push(start);
        } else {
            bucket.push(Group {
                starts: vec![start],
                length: end - start,
                meaningful: minimum,
            });
        }
    }

    let mut seeds: Vec<_> = seeds
        .into_values()
        .flatten()
        .filter(|group| {
            group.starts.len() >= config.min_occurrences
                && group
                    .starts
                    .iter()
                    .any(|&start| lines.query_end[start] >= start + group.length)
        })
        .collect();
    seeds.sort_unstable_by_key(|group| group.starts[0]);
    let mut found = Vec::new();
    for seed in seeds {
        // An earlier extended sequence can cover a later window at every site.
        // Independent extra occurrences keep the shorter seed alive.
        if found.iter().any(|longer: &Group| longer.covers(&seed)) {
            continue;
        }
        extend(&lines, seed, config.min_occurrences, &mut found);
    }

    groups::consolidate(&mut found);
    let mut diagnostics: Vec<_> = found
        .into_iter()
        .filter_map(|group| {
            let (anchor, sites) = select_sites(&group, &lines.query_end, config.min_occurrences)?;
            Some(diagnostic(&group, anchor, &sites))
        })
        .collect();
    diagnostics.sort_unstable_by(|a, b| (a.line, &a.message).cmp(&(b.line, &b.message)));
    diagnostics
}

/// Render one textual group, including the query once and all other selected sites.
fn diagnostic(group: &Group, anchor: usize, sites: &[usize]) -> Diagnostic {
    let mut message = format!(
        "{} meaningful lines repeat at {} non-overlapping sites in this file. Locations: ",
        group.meaningful,
        sites.len()
    );
    for (index, start) in sites.iter().enumerate() {
        if index != 0 {
            message.push_str(", ");
        }
        let _ = write!(message, "{}-{}", start + 1, start + group.length);
    }
    message.push_str(
        ".\n\nWhy:\n- Less repeated code is easier to audit and keep consistent.\n\nSuggestions:\n- Consider a constructor or helper if it preserves behavior, contracts, and performance.\n- A useful refactor may not clear this reminder. That is OK; do not force further changes just to silence it."
    );

    Diagnostic {
        severity: Severity::Reminder,
        code: CODE_DUPLICATION,
        title: None,
        message,
        line: anchor + 1,
        item_kind: "source sequence".into(),
        item_name: None,
    }
}

/// Extend each branch only while a complete eligible occurrence still qualifies.
fn extend(lines: &Lines<'_>, seed: Group, minimum_sites: usize, found: &mut Vec<Group>) {
    let mut pending = vec![seed];
    while let Some(group) = pending.pop() {
        let Some((_, sites)) = select_sites(&group, &lines.query_end, minimum_sites) else {
            continue;
        };

        let mut branches: AHashMap<&[usize], Vec<usize>> = AHashMap::new();
        for &start in &group.starts {
            let end = start + group.length;
            if let Some(next) = lines.next_meaningful(end) {
                branches
                    .entry(&lines.ids[end..=next])
                    .or_default()
                    .push(start);
            }
        }
        let mut children: Vec<_> = branches
            .into_iter()
            .filter_map(|(tail, starts)| {
                let child = Group {
                    starts,
                    length: group.length + tail.len(),
                    meaningful: group.meaningful + 1,
                };
                select_sites(&child, &lines.query_end, minimum_sites).map(|_| child)
            })
            .collect();
        children.sort_unstable_by_key(|child| child.starts[0]);

        // Retain a shorter sequence if extension loses an independent site.
        let covered = children.iter().any(|child| {
            select_sites(child, &lines.query_end, minimum_sites).is_some_and(|(_, longer_sites)| {
                sites
                    .iter()
                    .all(|start| longer_sites.binary_search(start).is_ok())
            })
        });
        if !covered {
            // Output groups need independent sites, not every overlapping window.
            found.push(Group {
                starts: sites,
                ..group
            });
        }
        pending.extend(children.into_iter().rev());
    }
}
