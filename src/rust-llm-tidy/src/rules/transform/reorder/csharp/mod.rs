//! Reorder C# declarations without crossing preprocessor boundaries.

use super::Permutation;
use super::graph::compute_member_order;
use crate::languages::csharp::regions::Regions;
use crate::source::{ItemKind, ParseResult};
use profile::CSharpProfile;

mod profile;

/// Compute the full reorder permutation for a parsed `.cs` file: the
/// top-level item order plus one member permutation per type or namespace
/// body.
///
/// Returns `Ok(None)` when the source holds constructs the engine
/// declines to reorder: parse-tree error nodes, a preprocessor region
/// scan that rejects the source, two top-level declarations sharing
/// a row, or CR-styled line endings.
///
/// The sharing-a-row and line-ending guards keep the span tiling
/// honest: a degenerate span has no representable slice.
///
/// The region scan runs twice - once in the parse to stamp ids, once
/// here as the authority for degradation - because the shared parse
/// result carries no ambiguity flag; the scan is one linear pass, far
/// below the tree parse it accompanies.
pub(crate) fn reorder_permutation(parsed: &ParseResult) -> anyhow::Result<Option<Permutation>> {
    if parsed.syntax_tree().root_node().has_error() {
        return Ok(None);
    }
    if Regions::scan(&parsed.source).is_none() {
        return Ok(None);
    }
    // A carriage return that is not part of a CRLF pair (CR-styled
    // line endings) is outside the span model's lexicon.
    if parsed
        .source
        .bytes()
        .enumerate()
        .any(|(i, b)| b == b'\r' && parsed.source.as_bytes().get(i + 1) != Some(&b'\n'))
    {
        return Ok(None);
    }
    // A top-level pair sharing a row (either declaration's span
    // reaching the next one's start row) is unrepresentable for the
    // span tiling: the later item's span degenerates.
    //
    // Degrade to a no-op rather than emitting a guessed rewrite or a
    // record for a move the bytes never perform.
    {
        let root = parsed.syntax_tree().root_node();
        let mut cursor = root.walk();
        let decls: Vec<_> = root
            .children(&mut cursor)
            .filter(|n| n.is_named() && n.kind() != "comment")
            .collect();
        if decls
            .windows(2)
            .any(|pair| pair[0].end_position().row >= pair[1].start_position().row)
        {
            return Ok(None);
        }
    }

    let order = top_level_order(parsed);
    let mut permutation = Permutation::new(parsed.items.len(), order)?;

    // Pair every item with its declaration node: the parse builds exactly
    // one item per non-comment compilation-unit child, in order.
    let source = parsed.source.as_str();
    let root = parsed.syntax_tree().root_node();
    let mut top = root.walk();
    let mut decls = root
        .children(&mut top)
        .filter(|n| n.is_named() && n.kind() != "comment");
    for (idx, item) in parsed.items.iter().enumerate() {
        let Some(node) = decls.next() else {
            break;
        };
        let members = item.members();
        if members.len() < 2 {
            continue;
        }
        let Some(body) = node
            .child_by_field_name("body")
            .filter(|b| b.kind() == "declaration_list")
        else {
            continue;
        };
        let mut member_cursor = body.walk();
        let member_decls: Vec<_> = body
            .children(&mut member_cursor)
            .filter(|n| n.is_named() && n.kind() != "comment")
            .collect();
        // Only the method bucket dependency-sorts, so a body with fewer
        // than two methods cannot consume an edge: skip its reference walk.
        let method_count = members.iter().filter(|m| *m.kind() == ItemKind::Fn).count();
        let edges = if method_count >= 2 {
            profile::member_edges(&member_decls, members, source)
        } else {
            Vec::new()
        };
        let member_order = compute_member_order(members, &edges, &CSharpProfile);
        permutation.set_member_order(idx, members.len(), member_order)?;
    }

    Ok(Some(permutation))
}

/// The top-level item order for the all-stable C# profile: region runs
/// emit in source order, and within each run the `using` directives pin
/// first while everything else keeps source order.
///
/// The engine's [`compute_order`] would produce the same order, but it
/// unconditionally collects reference edges for dependency phases this
/// profile never uses; deriving the order directly skips that discarded
/// full-tree walk.
///
/// [`compute_order`]: crate::rules::transform::reorder::graph::compute_order
fn top_level_order(parsed: &ParseResult) -> Vec<usize> {
    let mut order = Vec::with_capacity(parsed.items.len());
    let mut run_start = 0;
    while run_start < parsed.items.len() {
        let region = parsed.items[run_start].region();
        let mut run_end = run_start + 1;
        while run_end < parsed.items.len() && parsed.items[run_end].region() == region {
            run_end += 1;
        }
        for pick_using in [true, false] {
            for idx in run_start..run_end {
                let is_using = parsed.items[idx].kind() == &ItemKind::Using;
                if is_using == pick_using {
                    order.push(idx);
                }
            }
        }
        run_start = run_end;
    }
    order
}
