//! MOD003 full-namespace C# advice and hint-only exit behavior.

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
#[case::attributes("[global::Vendor.Marker] class C {}")]
#[case::global_attributes("[assembly: global::Vendor.Marker] class C {}")]
#[case::ambiguous("using X = global::A.X; using X = global::B.X; class C { global::A.X field; }")]
fn cli_should_exempt_non_code_and_ambiguous_paths(#[case] source: &str) {
    let (stderr, exit) = run_qualified_path_source(source, "cs");

    assert_eq!(exit, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

/// Partial paths, aliases, and uncertain roots stay silent at any depth.
#[rstest]
#[case::partial_type("using System; class C { Threading.Tasks.Task field; }")]
#[case::partial_expression(
    "using System; class C { void M() { Threading.Tasks.Task.Factory.StartNew(); } }"
)]
#[case::unknown_type("class C { Vendor.Net.Client field; }")]
#[case::unknown_import("using Vendor.Net; class C { Vendor.Net.Client field; }")]
#[case::alias("using System = Vendor.Net; class C { System.Client field; }")]
#[case::alias_qualified("using S = System; class C { S::Threading.Tasks.Task field; }")]
#[case::relative_namespace(
    "namespace Work.System {} namespace Work { class C { System.Threading.Tasks.Task field; } }"
)]
#[case::receiver_shadow("class C { void M(object System) { System.Console.Out.WriteLine(1); } }")]
#[case::generic_type("class C { global::System.Collections.Generic.List<int> field; }")]
#[case::generic_method("class C { void M() { global::System.Array.Empty<int>(); } }")]
fn cli_should_exempt_paths_without_reliable_full_namespace_advice(#[case] source: &str) {
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
    "class C { global::Vendor.Net.Client field; }",
    "global::Vendor.Net.Client",
    "Add `using Client = global::Vendor.Net.Client;`",
    "Client"
)]
#[case::absolute_expression(
    "class C { void M(object System) { global::System.Console.Out.WriteLine(1); } }",
    "global::System.Console.Out.WriteLine",
    "Add `using Console = global::System.Console;`",
    "Console.Out.WriteLine"
)]
#[case::absolute_alias(
    "using Log = global::System.Console; class C { void M() { global::System.Console.WriteLine(1); } }",
    "global::System.Console.WriteLine",
    "`Log` is already imported.",
    "Log.WriteLine"
)]
#[case::long_full_path(
    "class C { void M() { Microsoft.Win32.Registry.CurrentUser.OpenSubKey(); } }",
    "Microsoft.Win32.Registry.CurrentUser.OpenSubKey",
    "Add `using Win32 = Microsoft.Win32;`",
    "Win32.Registry.CurrentUser.OpenSubKey"
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
        stderr.contains(&format!("path `{path}` includes the full namespace.")),
        "{stderr}"
    );
    assert!(stderr.contains(import_advice), "{stderr}");
    assert!(
        stderr.contains(&format!("- Replace this path with `{replacement}`")),
        "{stderr}"
    );
}
