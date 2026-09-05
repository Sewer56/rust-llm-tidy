//! C# backend tests over the crate fixtures: parse shape, region
//! interaction, reorder composition, and lint semantics.
//!
//! Everything drives the public backend API ([`backend_for`]) the way the
//! CLI pipeline does, with fixture sources under `tests/fixtures/csharp/`.

use rust_llm_tidy_lang::LanguageBackend;
use rust_llm_tidy_lint::Severity;
use rust_llm_tidy_model::parse::ItemKind;
use rust_llm_tidy_reorder::reorder::emit;

/// A body that opens with blank lines still reorders: the blank lines
/// travel with the first member, so the profile applies exactly as it
/// does without them.
#[test]
fn blank_lines_after_the_opening_brace_still_reorder() {
    let class_source = concat!(
        "class C\n",
        "{\n",
        "\n",
        "    void M() { }\n",
        "    int F;\n",
        "}\n",
    );
    let parsed = parse(class_source);
    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("composition must succeed")
        .expect("blank lines after the brace stay tileable");
    let output = emit(&parsed, &permutation).expect("emit must succeed");

    let field = output.find("int F;").expect("field survives");
    let method = output.find("void M()").expect("method survives");
    assert!(
        field < method,
        "the field hoists ahead of the method:\n{output}"
    );
    assert!(
        output.contains("int F;\n\n    void M()"),
        "the blank line travels with the member it preceded:\n{output}"
    );

    let namespace_source = concat!(
        "namespace N\n",
        "{\n",
        "\n",
        "    class C { }\n",
        "    using System;\n",
        "}\n",
    );
    let parsed = parse(namespace_source);
    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("namespace composition must succeed")
        .expect("blank lines after the brace stay tileable");
    let output = emit(&parsed, &permutation).expect("emit must succeed");

    let using = output.find("using System;").expect("using survives");
    let class = output.find("class C").expect("class survives");
    assert!(
        using < class,
        "the nested using hoists above the types:\n{output}"
    );
}

/// A type body whose members do not each occupy their own lines keeps its
/// member order entirely: line-tiled spans cannot represent the body, so
/// the emitted output equals the source.
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
/// region ids (their first line is a directive line), so each forms a
/// singleton region run that never moves; the whole fixture reorders to
/// itself.
#[test]
fn conditional_items_never_move_and_the_fixture_is_a_noop() {
    let source = include_str!("fixtures/csharp/region_fixture.cs");
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
    let source = include_str!("fixtures/csharp/region_fixture.cs");
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

/// DOC001 fires on undocumented non-private members of every documentable
/// kind and stays silent for private and documented ones.
#[test]
fn doc001_flags_undocumented_non_private_members() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    public void Public() { }\n",
        "    internal int Internal { get; set; }\n",
        "    protected void Protected() { }\n",
        "    private void Private() { }\n",
        "    void Default() { }\n",
        "    /// <summary>Documented.</summary>\n",
        "    public void Documented() { }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC001");

    assert!(
        found.contains(&"4:Public".to_string()),
        "public method: {found:?}"
    );
    assert!(
        found.contains(&"5:Internal".to_string()),
        "internal property: {found:?}"
    );
    assert!(
        found.contains(&"6:Protected".to_string()),
        "protected method: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("Private")),
        "private: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("Default")),
        "no modifier: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("Documented")),
        "documented: {found:?}"
    );
    assert_eq!(
        found.len(),
        3,
        "exactly the three non-private gaps: {found:?}"
    );
}

/// `nameof(X)` mentions a same-file throwing member without calling it,
/// so it is never a call edge.
#[test]
fn doc002_does_not_treat_nameof_as_a_call() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Throws.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Always.</exception>\n",
        "    private void Thrower() { throw new System.InvalidOperationException(); }\n",
        "\n",
        "    /// <summary>Mentions the thrower.</summary>\n",
        "    public string Mention() { return nameof(Thrower); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert!(
        found.is_empty(),
        "nameof references, never calls: {found:?}"
    );
}

/// DOC002 errors when a documented non-private member throws without an
/// `<exception>` tag; an `<exception>` presence silences it. Constructors
/// with bodies scan like methods.
#[test]
fn doc002_errors_on_untagged_throwers() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>No tag.</summary>\n",
        "    public void Missing() { throw new System.Exception(); }\n",
        "\n",
        "    /// <summary>Untagged constructor.</summary>\n",
        "    public C(int seed) { throw new System.Exception(); }\n",
        "\n",
        "    /// <summary>Tagged.</summary>\n",
        "    /// <exception cref=\"System.Exception\">On failure.</exception>\n",
        "    public void Tagged() { throw new System.Exception(); }\n",
        "\n",
        "    /// <summary>Not throwing.</summary>\n",
        "    public void Clean() { }\n",
        "\n",
        "    /// <summary>Private throw.</summary>\n",
        "    private void Hidden() { throw new System.Exception(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert_eq!(
        found,
        vec!["4:Missing", "7:C"],
        "the untagged method and constructor only: {found:?}"
    );
    assert!(
        backend()
            .lint(&parsed)
            .iter()
            .filter(|d| d.code == "DOC002")
            .all(|d| d.severity == Severity::Error),
        "every DOC002 finding is error-severity"
    );
}

/// A private thrower stays silent on itself while its undocumented
/// non-private caller is flagged.
#[test]
fn doc002_flags_callers_of_private_throwers() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Calls the helper.</summary>\n",
        "    public void Caller() { Hidden(); }\n",
        "\n",
        "    /// <summary>Private thrower.</summary>\n",
        "    private void Hidden() { throw new System.InvalidOperationException(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert_eq!(found, vec!["4:Caller"], "the caller only: {found:?}");
}

/// DOC002 recursion: calling a same-file thrower flags the caller.
/// Every call form resolves by simple name - bare, `this.`-qualified,
/// receiver-qualified, generic - and a concrete-`cref` tag passes.
#[test]
fn doc002_flags_indirect_throwers_across_call_forms() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Throws.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Always.</exception>\n",
        "    private void Thrower() { throw new System.InvalidOperationException(); }\n",
        "\n",
        "    /// <summary>Throws.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Always.</exception>\n",
        "    private void Pair<T>() { throw new System.InvalidOperationException(); }\n",
        "\n",
        "    /// <summary>Bare call.</summary>\n",
        "    public void Missing() { Thrower(); }\n",
        "\n",
        "    /// <summary>Call through this.</summary>\n",
        "    public void ViaThis() { this.Thrower(); }\n",
        "\n",
        "    /// <summary>Call through a receiver.</summary>\n",
        "    public void ViaReceiver() { var other = this; other.Thrower(); }\n",
        "\n",
        "    /// <summary>Generic call.</summary>\n",
        "    public void ViaGeneric() { Pair<int>(); }\n",
        "\n",
        "    /// <summary>Documents the reached throw.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Via helper.</exception>\n",
        "    public void Tagged() { Thrower(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert_eq!(
        found,
        vec![
            "12:Missing",
            "15:ViaThis",
            "18:ViaReceiver",
            "21:ViaGeneric"
        ],
        "every call form reaches the thrower; the tagged caller passes: {found:?}"
    );
}

/// A recursion cycle whose members can reach a throw flags every member
/// of the cycle, and the closure over the cycle terminates.
#[test]
fn doc002_flags_mutually_recursive_callers_that_reach_a_throw() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Cycle half.</summary>\n",
        "    public int Walk(int n) { return n == 0 ? Fire() : Step(n - 1); }\n",
        "\n",
        "    /// <summary>Cycle half.</summary>\n",
        "    public int Step(int n) { return n == 0 ? Fire() : Walk(n - 1); }\n",
        "\n",
        "    /// <summary>Throws.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Always.</exception>\n",
        "    private int Fire() { throw new System.InvalidOperationException(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert_eq!(
        found,
        vec!["4:Walk", "7:Step"],
        "both cycle halves reach the throw: {found:?}"
    );
}

/// Throw detection is transitive: with First calling Second, Second
/// calling Third, and Third throwing, every undocumented member of the
/// chain is flagged, in document order.
#[test]
fn doc002_propagates_through_transitive_call_chains() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Entry.</summary>\n",
        "    public void First() { Second(); }\n",
        "\n",
        "    /// <summary>Middle.</summary>\n",
        "    public void Second() { Third(); }\n",
        "\n",
        "    /// <summary>Throws.</summary>\n",
        "    public void Third() { throw new System.InvalidOperationException(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert_eq!(
        found,
        vec!["4:First", "7:Second", "10:Third"],
        "the whole chain is flagged in document order: {found:?}"
    );
}

/// `new C()` resolves to the same-file constructor named `C`, so a
/// member building a throwing type is flagged alongside the throwing
/// constructor itself.
#[test]
fn doc002_resolves_object_creation_to_same_file_constructors() {
    let source = concat!(
        "/// <summary>A widget.</summary>\n",
        "public class Widget\n",
        "{\n",
        "    /// <summary>Constructor that throws.</summary>\n",
        "    public Widget() { throw new System.InvalidOperationException(); }\n",
        "\n",
        "    /// <summary>Builds through the throwing constructor.</summary>\n",
        "    public static Widget Create() { return new Widget(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert_eq!(
        found,
        vec!["4:Widget", "7:Create"],
        "the throwing constructor and its caller: {found:?}"
    );
}

/// Self- and mutual recursion that never reach a throw stay silent and
/// terminate.
#[test]
fn doc002_stays_silent_on_recursion_that_never_reaches_a_throw() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Self-recursive.</summary>\n",
        "    public int Loop(int n) { return n == 0 ? 0 : Loop(n - 1); }\n",
        "\n",
        "    /// <summary>Mutual half.</summary>\n",
        "    public int Ping(int n) { return n == 0 ? 0 : Pong(n - 1); }\n",
        "\n",
        "    /// <summary>Mutual half.</summary>\n",
        "    public int Pong(int n) { return n == 0 ? 0 : Ping(n - 1); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert!(found.is_empty(), "no throw is reachable: {found:?}");
}

/// Calls that resolve outside the file - framework members and helpers
/// from other files - never flag their caller.
#[test]
fn doc002_stays_silent_when_calls_resolve_outside_the_file() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Calls the framework.</summary>\n",
        "    public int ParseIt() { return int.Parse(\"1\"); }\n",
        "\n",
        "    /// <summary>Calls another file's helper.</summary>\n",
        "    public void Away() { ExternalHelper(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC002");

    assert!(
        found.is_empty(),
        "unresolvable calls stay silent: {found:?}"
    );
}

/// DOC003 follows the recursive gate: a caller with no `throw` of its
/// own whose only `<exception>` tag lacks a `cref` is warned about, not
/// silent.
#[test]
fn doc003_warns_on_indirect_throwers_with_vague_crefs() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Throws.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Always.</exception>\n",
        "    private void Thrower() { throw new System.InvalidOperationException(); }\n",
        "\n",
        "    /// <summary>Calls the thrower, vaguely.</summary>\n",
        "    /// <exception>On failure.</exception>\n",
        "    public void Vague() { Thrower(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC003");

    assert_eq!(found, vec!["8:Vague"], "the indirect caller: {found:?}");
    assert!(
        backend()
            .lint(&parsed)
            .iter()
            .filter(|d| d.code == "DOC003")
            .all(|d| d.severity == Severity::Warning),
        "indirect DOC003 findings keep warning severity"
    );
}

/// DOC003 warns when `<exception>` tags exist but none carries a concrete
/// `cref`; any non-empty `cref` satisfies it.
#[test]
fn doc003_warns_on_vague_exception_crefs() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Vague.</summary>\n",
        "    /// <exception>On failure.</exception>\n",
        "    public void Vague() { throw new System.Exception(); }\n",
        "    /// <summary>Concrete.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">No state.</exception>\n",
        "    public void Concrete() { throw new System.Exception(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC003");

    assert_eq!(
        found,
        vec!["4:Vague"],
        "only the cref-less tag set: {found:?}"
    );
}

/// DOC004 fires when a parameterized documented member has no `<param>`
/// tags at all; DOC005 when the tags omit a declared parameter.
/// Constructors and indexers carry real parameter lists into both checks.
#[test]
fn doc004_and_doc005_check_param_tags_against_real_parameters() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Untagged.</summary>\n",
        "    public void Untagged(string key, int count) { }\n",
        "\n",
        "    /// <summary>Partial.</summary>\n",
        "    /// <param name=\"key\">The key.</param>\n",
        "    public void Partial(string key, int count) { }\n",
        "\n",
        "    /// <summary>Complete.</summary>\n",
        "    /// <param name=\"key\">The key.</param>\n",
        "    /// <param name=\"count\">The count.</param>\n",
        "    public void Complete(string key, int count) { }\n",
        "\n",
        "    /// <summary>Untagged constructor.</summary>\n",
        "    public C(string seed) { }\n",
        "\n",
        "    /// <summary>Untagged indexer.</summary>\n",
        "    public int this[string key] => 0;\n",
        "}\n",
    );
    let parsed = parse(source);
    let doc004 = codes(&parsed, "DOC004");
    let doc005 = codes(&parsed, "DOC005");

    assert_eq!(
        doc004,
        vec!["4:Untagged", "16:C", "19:"],
        "methods, constructors, and indexers without tags: {doc004:?}"
    );
    assert_eq!(
        doc005,
        vec!["7:Partial"],
        "only the partial method: {doc005:?}"
    );
    let messages: Vec<String> = backend()
        .lint(&parsed)
        .into_iter()
        .filter(|d| d.code == "DOC005")
        .map(|d| d.message)
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("`count`")),
        "DOC005 must name the undocumented parameter: {messages:?}"
    );
}

/// DOC006 fires on TODO/FIXME/TBD whole words in doc comments, not inside
/// longer words.
#[test]
fn doc006_flags_placeholder_words_only() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>TODO: write this.</summary>\n",
        "    public void Todo() { }\n",
        "    /// <summary>Mentions todolist shapes.</summary>\n",
        "    public void Todolist() { }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC006");

    assert_eq!(found, vec!["4:Todo"], "whole words only: {found:?}");
}

// ── Doc-comment attachment through reorder ───────────────────────

/// A `///` doc run above the first item stays attached to that item
/// even under a plain `//` banner: a hoisted `using` lands after the
/// banner (the banner stays in the preamble) and before the item's doc
/// run.
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

// ── Region interaction ───────────────────────────────────────────

/// A block-scoped namespace body reorders like a type body: the nested
/// `using` hoists above the namespace's types while the types keep their
/// order.
#[test]
fn namespace_body_hoists_nested_usings_above_its_types() {
    let source = concat!(
        "namespace Demo\n",
        "{\n",
        "    public class Service { }\n",
        "\n",
        "    using System.IO;\n",
        "}\n",
    );
    let parsed = parse(source);
    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("composition must succeed")
        .expect("fixture holds no unsupported construct");
    let output = emit(&parsed, &permutation).expect("emit must succeed");

    let using = output.find("using System.IO;").expect("using survives");
    let class = output.find("class Service").expect("class survives");
    assert!(
        using < class,
        "a nested using hoists above the types:\n{output}"
    );

    // The second composition of the emitted output is a fixpoint.
    let reparsed = parse(&output);
    let second = backend()
        .reorder_permutation(&reparsed)
        .expect("second composition must succeed")
        .expect("still no unsupported construct");
    assert_eq!(
        emit(&reparsed, &second).expect("second emit must succeed"),
        output,
        "a reordered namespace body composes to itself"
    );
}

/// A type nested inside a reordering body moves as one member and its
/// own members keep their source order even where a nested-body sort
/// would move them: the callee precedes its caller and the property
/// trails the methods inside the nested type.
#[test]
fn nested_type_moves_whole_while_the_enclosing_body_reorders() {
    let source = concat!(
        "public class Outer\n",
        "{\n",
        "    public void Run() { Apply(); }\n",
        "\n",
        "    void Apply() { }\n",
        "\n",
        "    public class Inner\n",
        "    {\n",
        "        public void Second() { }\n",
        "\n",
        "        public void First() { Second(); }\n",
        "\n",
        "        public int Tally { get; set; }\n",
        "    }\n",
        "\n",
        "    public int Count { get; set; }\n",
        "}\n",
    );
    let parsed = parse(source);
    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("composition must succeed")
        .expect("fixture holds no unsupported construct");
    let output = emit(&parsed, &permutation).expect("emit must succeed");

    // The enclosing body permutes: the nested type and the property
    // precede the methods.
    let count = output
        .find("Count { get; set; }")
        .expect("property survives");
    let run = output.find("Run()").expect("caller survives");
    let inner = output.find("class Inner").expect("nested type survives");
    assert!(count < run, "the outer body applies the profile:\n{output}");
    assert!(
        inner < count,
        "the nested type sits in the bucket ahead of the property:\n{output}"
    );

    // The nested type stays whole: its callee keeps its source position
    // before its caller, and its property stays after the methods -
    // both would move under any nested-body sort.
    let second = output.find("Second()").expect("nested callee survives");
    let first = output.find("First()").expect("nested caller survives");
    let tally = output
        .find("Tally { get; set; }")
        .expect("nested property survives");
    assert!(
        second < first,
        "nested members keep their source order:\n{output}"
    );
    assert!(
        first < tally,
        "the nested property stays after the methods:\n{output}"
    );

    let reparsed = parse(&output);
    let second_perm = backend()
        .reorder_permutation(&reparsed)
        .expect("second composition must succeed")
        .expect("still no unsupported construct");
    assert_eq!(
        emit(&reparsed, &second_perm).expect("second emit must succeed"),
        output,
        "the nested whole composes to itself"
    );
}

// ── Parse shape ──────────────────────────────────────────────────

/// The parse fixture classifies every top-level declaration: usings, the
/// file-scoped namespace, and the documented class, in document order,
/// with the class's members typed per the member table.
#[test]
fn parse_classifies_top_level_items_and_members() {
    let source = include_str!("fixtures/csharp/parse_fixture.cs");
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
    let source = include_str!("fixtures/csharp/parse_fixture.cs");
    let parsed = parse(source);

    let namespace = &parsed.items[2];
    assert_eq!(namespace.name(), Some("Fixtures"));
    let class = &parsed.items[3];
    assert_eq!(
        class.visibility(),
        Some(rust_llm_tidy_model::parse::VisibilityTier::Pub)
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
    let source = include_str!("fixtures/csharp/parse_fixture.cs");
    let parsed = parse(source);

    let preamble = &source[..parsed.preamble_end];
    assert!(preamble.contains("// License header"));
    assert!(!preamble.contains("using System;"));
}

/// Item spans tile back-to-back: each `end` is the byte after the item's
/// trailing newline and every later item's `start` is the previous
/// `end`, so reordering carries inter-item comments and blank lines.
///
/// The trailer starts exactly where the last item ends.
#[test]
fn parse_tiles_item_spans_back_to_back() {
    let source = include_str!("fixtures/csharp/parse_fixture.cs");
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

/// The reorder permutation orders members by the profile buckets with
/// callers before callees among methods, and `using` directives pin first
/// at both levels.
#[test]
fn reorder_permutation_applies_profile_and_caller_first() {
    let source = concat!(
        "using System;\n",
        "\n",
        "namespace N;\n",
        "\n",
        "public class C\n",
        "{\n",
        "    public void Caller() { Callee(); }\n",
        "\n",
        "    void Callee() { }\n",
        "\n",
        "    public int Prop { get; set; }\n",
        "\n",
        "    private int _field;\n",
        "}\n",
        "\n",
        "class Late { }\n",
        "using System.IO;\n",
    );
    let parsed = parse(source);
    let permutation = backend()
        .reorder_permutation(&parsed)
        .expect("composition must succeed")
        .expect("fixture holds no unsupported construct");
    let output = emit(&parsed, &permutation).expect("emit must succeed");

    // Top level: usings pin first (both of them, compact); everything
    // else keeps source order.
    let using_io = output.find("using System.IO;").expect("using survives");
    let namespace = output.find("namespace N;").expect("namespace survives");
    let late = output.find("class Late").expect("late class survives");
    assert!(using_io < namespace, "hoisted using precedes the namespace");
    assert!(namespace < late, "non-using items keep source order");

    // Body: field, property, then caller before callee.
    let field = output.find("_field;").expect("field survives");
    let prop = output.find("Prop {").expect("property survives");
    let caller = output.find("Caller()").expect("caller survives");
    let callee = output.find("Callee()").expect("callee survives");
    assert!(
        field < prop && prop < caller && caller < callee,
        "members order fields, properties, then callers before callees:\n{output}"
    );

    // Idempotent: the emitted output composes to itself.
    let reparsed = parse(&output);
    let second = backend()
        .reorder_permutation(&reparsed)
        .expect("second composition must succeed")
        .expect("still no unsupported construct");
    let twice = emit(&reparsed, &second).expect("second emit must succeed");
    assert_eq!(twice, output, "a reordered file must compose to itself");
}

/// Two top-level declarations on one row are unrepresentable for the
/// top-level tiling: the whole reorder declines to a no-op with zero
/// records and byte-stable output, whatever the profile order would do.
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

/// TEST001 fires for every accepted marker attribute on a discouraged
/// name and passes behavioral names.
#[test]
fn test001_flags_discouraged_names_with_any_marker() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    [TestMethod]\n",
        "    public void Test1() { }\n",
        "    [Test]\n",
        "    public void Test_foo() { }\n",
        "    [Fact]\n",
        "    public void Case_1() { }\n",
        "    [Theory]\n",
        "    public void Test() { }\n",
        "    [Fact]\n",
        "    public void ShouldReturnZeroWhenEmpty() { }\n",
        "    public void Test_undecorated() { }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "TEST001");

    // Lines point at each method's attribute line (attributes travel with
    // the declaration, matching the Rust item-line convention).
    assert_eq!(
        found,
        vec!["4:Test1", "6:Test_foo", "8:Case_1", "10:Test"],
        "marker + discouraged name pairs only: {found:?}"
    );
}

// ── Lint semantics ───────────────────────────────────────────────

/// Lint diagnostics for a source, filtered to one code.
fn codes(parsed: &rust_llm_tidy_model::parse::ParseResult, code: &str) -> Vec<String> {
    backend()
        .lint(parsed)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| format!("{}:{}", d.line, d.item_name.as_deref().unwrap_or("")))
        .collect()
}

/// Parse `source` through the backend.
fn parse(source: &str) -> rust_llm_tidy_model::parse::ParseResult {
    backend().parse(source).expect("fixture must parse")
}

/// The registered C# backend.
fn backend() -> &'static dyn LanguageBackend {
    rust_llm_tidy_lang::backend_for("cs").expect("cs must resolve a backend")
}
