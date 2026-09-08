//! MOD003 rendered C# advice and hint-only exit behavior.

use crate::run_qualified_path_source;
use rstest::rstest;

/// Comments and strings do not suppress real paths; ordinary members stay exempt.
#[rstest]
#[case::comment("// #if DEBUG\nSystem.Console.WriteLine(1);", 1)]
#[case::string("var text = \"#if DEBUG\"; System.Console.WriteLine(1);", 1)]
#[case::repeated("System.Console.WriteLine(1); System.Console.WriteLine(2);", 2)]
#[case::member("obj.Member.Open();", 0)]
#[case::shadowed("var Console = 1; System.Console.WriteLine(2);", 0)]
fn cli_should_count_eligible_paths(#[case] body: &str, #[case] expected: usize) {
    let source = format!("class C {{ void M() {{ {body} }} }}");

    let (stderr, exit) = run_qualified_path_source(&source, "cs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), expected, "{stderr}");
}

/// Conditional regions exempt entire methods while siblings remain checked.
#[rstest]
#[case::body("void M() { System.Console.WriteLine(1);\n#if DEBUG\nint x = 1;\n#endif\n}")]
#[case::method("\n#if DEBUG\nvoid M() { System.Console.WriteLine(1); }\n#endif\n")]
fn cli_should_exempt_conditional_method(#[case] guarded: &str) {
    let source =
        format!("class C {{ {guarded}\nvoid Sibling() {{ System.Console.WriteLine(2); }} }}");

    let (stderr, exit) = run_qualified_path_source(&source, "cs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), 1, "{stderr}");
    assert!(stderr.contains("(fn `Sibling`)"), "{stderr}");
}

/// A conditional enclosing type does not exempt an unrelated type.
#[test]
fn cli_should_exempt_conditionally_compiled_type() {
    let source = "#if DEBUG\nclass C { void M() { System.Console.WriteLine(1); } }\n#endif\nclass D { void Sibling() { System.Console.WriteLine(2); } }";

    let (stderr, exit) = run_qualified_path_source(source, "cs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), 1, "{stderr}");
    assert!(stderr.contains("(fn `Sibling`)"), "{stderr}");
}

/// Import text, attributes, and ambiguous aliases stay exempt through the CLI.
#[rstest]
#[case::imports("using System.Threading.Tasks;")]
#[case::attributes("[Vendor.Marker] class C {}")]
#[case::ambiguous("using X = A.X; using X = B.X; class C { A.X field; }")]
fn cli_should_exempt_non_code_and_ambiguous_paths(#[case] source: &str) {
    let (stderr, exit) = run_qualified_path_source(source, "cs");

    assert_eq!(exit, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

/// First occurrences render valid import and replacement advice.
#[rstest]
#[case::missing(
    "class C { void M() { System.Console.WriteLine(1); } }",
    "System.Console.WriteLine",
    "Add `using Console = System.Console;`",
    "Console.WriteLine"
)]
#[case::imported(
    "using System.Threading.Tasks; class C { void M() { System.Threading.Tasks.Task.Delay(1); } }",
    "System.Threading.Tasks.Task.Delay",
    "already imported.",
    "Task.Delay"
)]
#[case::alias(
    "using Log = System.Console; class C { void M() { System.Console.WriteLine(1); } }",
    "System.Console.WriteLine",
    "`Log` is already imported.",
    "Log.WriteLine"
)]
#[case::custom(
    "class C { Vendor.Net.Client field; }",
    "Vendor.Net.Client",
    "Add `using Client = Vendor.Net.Client;`",
    "Client"
)]
fn cli_should_render_first_occurrence_hint(
    #[case] source: &str,
    #[case] path: &str,
    #[case] import_advice: &str,
    #[case] replacement: &str,
) {
    let (stderr, exit) = run_qualified_path_source(source, "cs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), 1, "{stderr}");
    assert!(
        stderr.contains(&format!(
            "fully-qualified path `{path}` makes code harder to read."
        )),
        "{stderr}"
    );
    assert!(stderr.contains(import_advice), "{stderr}");
    assert!(
        stderr.contains(&format!("- Replace this path with `{replacement}`")),
        "{stderr}"
    );
}
