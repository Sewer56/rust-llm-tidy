//! Reorder boundaries: constructs that freeze members or decline the
//! whole reorder instead of moving anything.

use super::{backend, parse};
use rust_llm_tidy::rules::transform::reorder::emit;
use rust_llm_tidy::source::ItemKind;

/// A type body whose members do not each occupy their own lines keeps its
/// member order entirely.
///
/// Line-tiled spans cannot represent the body, so the emitted output
/// equals the source.
#[test]
fn bodies_with_same_line_members_stay_whole() {
    let cases = [
        ("C", "class C { void M() {} int F; }\n"),
        (
            "C",
            concat!("class C\n", "{\n", "    void M() { }\n", "    int F; }\n"),
        ),
        (
            "C",
            concat!("class C { void M() { }\n", "    int F;\n", "}\n"),
        ),
        (
            "N",
            concat!("namespace N { class C { }\n", "using System;\n", "}\n"),
        ),
        (
            "C",
            concat!("class C { \n", "    void M() { }\n", "    int F;\n", "}\n"),
        ),
        (
            "C",
            concat!(
                "class C { // note\n",
                "    void M() { }\n",
                "    int F;\n",
                "}\n"
            ),
        ),
    ];
    for (container, source) in cases {
        let parsed = parse(source);
        let members = parsed
            .items
            .iter()
            .find(|item| item.name() == Some(container))
            .expect("container must parse")
            .members();
        assert!(
            members.is_empty(),
            "same-line members must freeze the body: {source:?}"
        );

        let permutation = backend()
            .reorder_permutation(&parsed)
            .expect("composition must succeed")
            .expect("no unsupported construct to decline");
        assert_eq!(
            emit(&parsed, &permutation).expect("emit must succeed"),
            source,
            "a frozen body emits its original bytes: {source:?}"
        );
    }
}

/// Top-level conditionals parse as single opaque items with their own
/// region ids (their first line is a directive line).
///
/// Each forms a singleton region run that never moves. The whole fixture
/// reorders to itself.
#[test]
fn conditional_items_never_move_and_the_fixture_is_a_noop() {
    let source = include_str!("../fixtures/csharp/region_fixture.cs");
    let parsed = parse(source);

    // The conditional wraps DebugOnly into one opaque item: no item is
    // named DebugOnly, and that item sits in a region of its own.
    assert!(
        parsed
            .items
            .iter()
            .all(|item| item.name() != Some("DebugOnly")),
        "a conditional wraps its content into one opaque item"
    );
    let using_region = parsed
        .items
        .iter()
        .find(|item| item.kind() == &ItemKind::Using)
        .expect("the fixture has a using directive")
        .region();
    let conditional = parsed
        .items
        .iter()
        .find(|item| item.kind() == &ItemKind::Other)
        .expect("the conditional parses as an opaque item");
    assert_eq!(using_region, 0, "a plain-region using keeps region 0");
    assert_ne!(
        conditional.region(),
        using_region,
        "a conditional item sits in its own region"
    );

    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("the fixture must compose")
        .expect("the fixture holds no unsupported construct");
    let output = emit(&parsed, &permutation).expect("emit must succeed");
    assert_eq!(
        output, source,
        "nothing may move across or out of a conditional region"
    );
}

/// CR-styled line endings (a `\r` that is not part of a CRLF pair)
/// sit outside the span model: the whole reorder declines to a no-op.
#[test]
fn cr_styled_line_endings_decline_the_whole_reorder() {
    let cases = [
        concat!(
            "class C\n",
            "{\r\r\n",
            "    void M() { }\n",
            "    int F;\n",
            "}\n"
        ),
        concat!(
            "namespace N\n",
            "{\r\r\n",
            "    class C { }\r\r\n",
            "    using System;\r\r\n",
            "}\r\r\n"
        ),
    ];
    for source in cases {
        let parsed = parse(source);
        assert!(
            backend().reorder_permutation(&parsed).unwrap().is_none(),
            "CR-styled line endings must decline: {source:?}"
        );
    }
}

/// A body holding a preprocessor directive emits no members: the whole
/// body stays atomic, so no member can cross the conditional boundary.
#[test]
fn directive_inside_a_body_freezes_its_members() {
    let source = include_str!("../fixtures/csharp/region_fixture.cs");
    let parsed = parse(source);

    let mixed = parsed
        .items
        .iter()
        .find(|item| item.name() == Some("Mixed"))
        .expect("Mixed class must parse");
    assert!(
        mixed.members().is_empty(),
        "a body with a preprocessor directive must not emit members"
    );
}

// ── Doc-comment attachment through reorder ───────────────────────

/// A `///` doc run above the first item stays attached to that item
/// even under a plain `//` banner.
///
/// A hoisted `using` lands after the banner (the banner stays in the
/// preamble) and before the item's doc run.
///
/// The rewritten file lints clean for the documented item.
#[test]
fn first_item_doc_run_stays_attached_under_a_plain_banner() {
    let source = concat!(
        "// banner\n",
        "/// <summary>Thing.</summary>\n",
        "public class C { void M() { } }\n",
        "using System;\n",
    );
    let parsed = parse(source);
    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("composition must succeed")
        .expect("fixture holds no unsupported construct");
    let output = emit(&parsed, &permutation).expect("emit must succeed");

    let banner = output.find("// banner").expect("banner survives");
    let using = output.find("using System;").expect("using survives");
    let docs = output
        .find("/// <summary>Thing.</summary>")
        .expect("docs survive");
    let class = output.find("class C").expect("class survives");
    assert!(
        banner < using && using < docs && docs < class,
        "the hoisted using lands above the item's doc run:\n{output}"
    );

    // The rewritten file keeps the class documented: DOC001 stays silent
    // for it.
    let reparsed = parse(&output);
    assert!(
        backend()
            .lint(&reparsed)
            .iter()
            .all(|d| !d.message.contains("`C`")),
        "the tool's own rewrite must not strip the class docs: {:?}",
        backend().lint(&reparsed)
    );
}

/// A source whose region scan rejects (an interpolation hole holding a
/// string literal) degrades reordering to a no-op even though it parses.
#[test]
fn rejected_region_scan_degrades_reorder_to_a_noop() {
    let source = concat!(
        "class C\n",
        "{\n",
        "    string S { get; set; }\n",
        "    void M() { var a = $\"{f(\"inner\")}\"; }\n",
        "    int F { get; set; }\n",
        "}\n",
    );
    let parsed = parse(source);

    assert!(
        backend().reorder_permutation(&parsed).unwrap().is_none(),
        "an ambiguous preprocessor scan must decline reordering"
    );
}

/// Two top-level declarations on one row are unrepresentable for the
/// top-level tiling.
///
/// The whole reorder declines to a no-op with zero records and
/// byte-stable output, whatever the profile order would do.
#[test]
fn same_line_top_level_items_decline_the_whole_reorder() {
    let cases = [
        "class C { } using System;\n",
        "namespace N { } class C { }\n",
        "using A;\nusing B; using C;\n",
        concat!("class C {\n", "} using System;\n"),
        concat!(
            "namespace N\n",
            "{\n",
            "    class C { }\n",
            "} using System;\n"
        ),
        concat!(
            "class C {\n",
            "    void M() {}\n",
            "    int F;\n",
            "} class D { }\n"
        ),
    ];
    for source in cases {
        let parsed = parse(source);
        assert!(
            backend().reorder_permutation(&parsed).unwrap().is_none(),
            "a same-line top-level pair must decline: {source:?}"
        );
    }
}
