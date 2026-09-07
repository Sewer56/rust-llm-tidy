//! Successful reorder compositions: namespace hoisting, nested types,
//! blank lines, and the profile with callers first.

use super::{backend, parse};
use rust_llm_tidy::rules::transform::reorder::emit;

/// A body that opens with blank lines still reorders.
///
/// The blank lines travel with the first member. So the profile applies
/// exactly as it does without them.
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

/// A type nested inside a reordering body moves as one member.
///
/// - Its own members keep their source order even where a nested-body
///   sort would move them.
/// - The callee precedes its caller.
/// - The property trails the methods inside the nested type.
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
    // before its caller.
    //
    // Its property stays after the methods - both would move under any
    // nested-body sort.
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
