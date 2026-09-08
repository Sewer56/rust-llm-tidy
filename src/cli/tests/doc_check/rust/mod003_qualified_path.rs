//! MOD003 rendered Rust advice and hint-only exit behavior.

use crate::run_qualified_path_source;
use rstest::rstest;

/// Partial qualification stays silent at any path or body depth.
#[rstest]
#[case::module_import("use std::fs; fn f() { fs::create_dir_all(dir); }")]
#[case::alias("use std::fs as io; fn f() { io::create_dir_all(dir); }")]
#[case::deep_partial("use std::os; fn f() { os::unix::fs::symlink(a, b); }")]
#[case::relative("fn f() { self::storage::write(); super::storage::write(); }")]
#[case::unknown("fn f() { vendor::net::Client::new(); }")]
#[case::nested("mod inner { use std::fs; fn f() { { fs::create_dir_all(dir); } } }")]
fn cli_should_allow_partial_paths(#[case] source: &str) {
    let (stderr, exit) = run_qualified_path_source(source, "rs");

    assert_eq!(exit, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

/// Comments and strings do not suppress real paths; repeats each receive a hint.
#[rstest]
#[case::comment("// #[cfg(test)]\nstd::mem::drop(1);", 1)]
#[case::string("let text = \"#[cfg(test)]\"; std::mem::drop(1);", 1)]
#[case::repeated("std::mem::drop(1); std::mem::drop(2);", 2)]
#[case::macro_only("println!(\"{}\", std::mem::size_of::<u8>());", 0)]
#[case::shadowed("let drop = 1; std::mem::drop(2);", 0)]
fn cli_should_count_eligible_paths(#[case] body: &str, #[case] expected: usize) {
    let source = format!("fn f() {{ {body} }}");

    let (stderr, exit) = run_qualified_path_source(&source, "rs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), expected, "{stderr}");
}

/// Conditional functions are exempt while unrelated siblings remain checked.
#[rstest]
#[case::body("fn f() { std::mem::drop(1); #[cfg(unix)] let x = 1; }")]
#[case::function("#[cfg(unix)] fn f() { std::mem::drop(1); }")]
#[case::test_module("#[cfg(test)] mod tests { fn f() { std::mem::drop(1); } }")]
#[case::conditional_attribute("#[cfg_attr(test, ignore)] fn f() { std::mem::drop(1); }")]
#[case::body_conditional_attribute(
    "fn f() { std::mem::drop(1); #[cfg_attr(test, allow(unused))] let x = 1; }"
)]
fn cli_should_exempt_conditional_function(#[case] guarded: &str) {
    let source = format!("{guarded}\nfn sibling() {{ std::mem::drop(2); }}");

    let (stderr, exit) = run_qualified_path_source(&source, "rs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), 1, "{stderr}");
    assert!(stderr.contains("(fn `sibling`)"), "{stderr}");
}

/// Import text, attributes, and ambiguous imports stay exempt through the CLI.
#[rstest]
#[case::imports("use std::sync::Arc;")]
#[case::attributes("#[vendor::marker] fn f() {}")]
#[case::ambiguous("use crate::a::X; use crate::b::X; fn f() { crate::a::X; }")]
fn cli_should_exempt_non_code_and_ambiguous_paths(#[case] source: &str) {
    let (stderr, exit) = run_qualified_path_source(source, "rs");

    assert_eq!(exit, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

/// A module-qualified replacement must not receive another hint.
#[test]
fn cli_should_only_flag_full_path_when_module_import_exists() {
    let source = "use std::fs; fn f() { std::fs::create_dir_all(dir); fs::create_dir_all(dir); }";

    let (stderr, exit) = run_qualified_path_source(source, "rs");

    assert_eq!(exit, 0, "{stderr}");
    assert_eq!(stderr.matches("hint[MOD003]").count(), 1, "{stderr}");
    assert!(stderr.contains("path `std::fs::create_dir_all` includes the full namespace."));
    assert!(
        stderr.contains("Replace this path with `fs::create_dir_all`; `fs` is already imported.")
    );
}

/// First occurrences identify full qualification and provide import advice.
#[rstest]
#[case::missing(
    "fn f() { std::sync::Arc::new(1); }",
    "std::sync::Arc::new",
    "Add `use std::sync::Arc;`",
    "Arc::new"
)]
#[case::imported(
    "use std::sync::Arc; fn f() { std::sync::Arc::new(1); }",
    "std::sync::Arc::new",
    "`Arc` is already imported.",
    "Arc::new"
)]
#[case::alias(
    "use std::sync::Arc as Shared; fn f() { std::sync::Arc::new(1); }",
    "std::sync::Arc::new",
    "`Shared` is already imported.",
    "Shared::new"
)]
#[case::custom(
    "fn f() { ::vendor::net::Client::new(); }",
    "::vendor::net::Client::new",
    "Add `use ::vendor::net::Client;`",
    "Client::new"
)]
fn cli_should_render_first_occurrence_hint(
    #[case] source: &str,
    #[case] path: &str,
    #[case] import_advice: &str,
    #[case] replacement: &str,
) {
    let (stderr, exit) = run_qualified_path_source(source, "rs");

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
