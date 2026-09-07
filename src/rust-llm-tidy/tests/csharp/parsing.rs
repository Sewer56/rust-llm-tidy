//! Parse-shape tests: item classification, spans, and error degradation
//! over the C# backend.

use super::{backend, parse};
use rust_llm_tidy::source::ItemKind;

// ── Parse shape ──────────────────────────────────────────────────

/// The parse fixture classifies every top-level declaration: usings,
/// the file-scoped namespace, and the documented class.
///
/// They appear in document order. The class's members are typed per
/// the member table.
#[test]
fn parse_classifies_top_level_items_and_members() {
    let source = include_str!("../fixtures/csharp/parse_fixture.cs");
    let parsed = parse(source);

    let kinds: Vec<ItemKind> = parsed.items.iter().map(|i| *i.kind()).collect();
    assert_eq!(
        kinds,
        [
            ItemKind::Using,
            ItemKind::Using,
            ItemKind::Namespace,
            ItemKind::Class
        ]
    );

    let class = &parsed.items[3];
    assert_eq!(class.name(), Some("ConfigLoader"));
    assert_eq!(class.doc_comments().len(), 3);

    let member_kinds: Vec<ItemKind> = class.members().iter().map(|m| *m.kind()).collect();
    assert_eq!(
        member_kinds,
        [ItemKind::Const, ItemKind::Constructor, ItemKind::Fn]
    );
    assert_eq!(class.members()[1].name(), Some("ConfigLoader"));
    assert_eq!(class.members()[2].name(), Some("Read"));
}

/// A source with parse-tree error nodes degrades reordering and the lint
/// pass to no-ops: no records, no findings.
#[test]
fn parse_errors_degrade_reorder_and_lints_to_noops() {
    let source = "class Broken { void M( { ))) }\n";
    let parsed = parse(source);

    assert!(backend().reorder_permutation(&parsed).unwrap().is_none());
    assert!(
        backend().lint(&parsed).is_empty(),
        "error-recovered trees must not produce findings"
    );
}

/// The class item carries `public` visibility and the full XML doc text;
/// the namespace item carries its name.
#[test]
fn parse_extracts_visibility_and_doc_text() {
    let source = include_str!("../fixtures/csharp/parse_fixture.cs");
    let parsed = parse(source);

    let namespace = &parsed.items[2];
    assert_eq!(namespace.name(), Some("Fixtures"));
    let class = &parsed.items[3];
    assert_eq!(
        class.visibility(),
        Some(rust_llm_tidy::source::VisibilityTier::Pub)
    );
    assert_eq!(
        class.doc_comments()[0],
        " <summary>Loads configuration values.</summary>",
        "doc entries keep the text after ///"
    );
}

/// Leading plain comments stay in the preamble; the first item's `///`
/// docs travel with the item, so the preamble ends at the doc run.
#[test]
fn parse_keeps_plain_comments_in_the_preamble() {
    let source = include_str!("../fixtures/csharp/parse_fixture.cs");
    let parsed = parse(source);

    let preamble = &source[..parsed.preamble_end];
    assert!(preamble.contains("// License header"));
    assert!(!preamble.contains("using System;"));
}

/// Item spans tile back-to-back: each `end` is the byte after the item's
/// trailing newline.
///
/// Every later item's `start` is the previous `end`, so reordering
/// carries inter-item comments and blank lines.
///
/// The trailer starts exactly where the last item ends.
#[test]
fn parse_tiles_item_spans_back_to_back() {
    let source = include_str!("../fixtures/csharp/parse_fixture.cs");
    let parsed = parse(source);

    for pair in parsed.items.windows(2) {
        assert_eq!(
            pair[1].start, pair[0].end,
            "item spans must tile without gaps or overlap"
        );
    }
    assert_eq!(
        parsed.trailer_start,
        parsed.items.last().expect("fixture has items").end,
        "the trailer starts where the last item ends"
    );
    assert!(source.ends_with(&source[parsed.trailer_start..]));
}
