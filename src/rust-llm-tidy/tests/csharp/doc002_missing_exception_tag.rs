//! Same-file DOC002 call resolution: untagged throwers, call forms,
//! recursion cycles, calls that resolve outside the file, and the
//! `nameof` carve-out.

use super::{backend, codes, parse};
use rust_llm_tidy::reporting::Severity;

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
/// `<exception>` tag; an `<exception>` presence silences it.
///
/// Constructors with bodies scan like methods.
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
///
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

/// Throw detection is transitive.
///
/// With First calling Second, Second calling Third, and Third throwing,
/// every undocumented member of the chain is flagged, in document order.
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
/// always stop.
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
