//! MOD001 module-header exclusion acceptance through the CLI.

use super::mod001_module_size::run;
use rstest::rstest;

// Module-header exclusion.

/// The default budget keeps each language's module header out of the
/// count; the warning's line keeps its physical position.
#[rstest]
#[case::rust_module_docs(
    "source.rs",
    "//! Docs.\n",
    " outside `#[cfg(test)]` mod regions and module headers,"
)]
#[case::rust_file_comments(
    "source.rs",
    "// Note.\n",
    " outside `#[cfg(test)]` mod regions and module headers,"
)]
#[case::python_docstring("source.py", "\"\"\"Doc.\"\"\"\n", " outside module headers,")]
#[case::python_comments("source.py", "# Note.\n", " outside module headers,")]
#[case::javascript("source.js", "// Note.\n", " outside module headers,")]
#[case::csharp("source.cs", "// Note.\n", " outside module headers,")]
#[case::shell("source.sh", "# Note.\n", " outside module headers,")]
fn mod001_should_exclude_module_headers_by_default(
    #[case] path: &str,
    #[case] header: &str,
    #[case] exclusions: &str,
) {
    let source = format!("{header}value = 1\nvalue = 2\n");

    let output = run(
        &source,
        path,
        "module_size:\n  max_lines: 1\n",
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(":3: warning[MOD001]"), "{stderr}");
    assert!(
        stderr.contains(&format!("file has 2 lines{exclusions}\n")),
        "{stderr}"
    );
}

/// Omitted inline-test configuration preserves Rust test-module exclusion.
#[rstest]
#[case::source("source.rs")]
#[case::integration_tests("tests/source.rs")]
fn mod001_should_exclude_rust_test_modules_by_default(#[case] path: &str) {
    let output = run(
        "fn value() {}\n#[cfg(test)]\nmod tests {\n    fn example() {}\n}\n",
        path,
        "module_size:\n  max_lines: 1\n",
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("MOD001"), "{stderr}");
}

/// Rust warnings explain the header exclusion exactly when a header was
/// excluded: the guidance gains its bullet only then.
#[rstest]
#[case::with_header("//! Docs.\nfn value() {}\nfn other() {}\n", true)]
#[case::without_header("fn value() {}\nfn other() {}\n", false)]
fn mod001_should_explain_module_header_exclusion_when_present(
    #[case] source: &str,
    #[case] explains: bool,
) {
    let output = run(
        source,
        "source.rs",
        "module_size:\n  max_lines: 1\n",
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("warning[MOD001]"), "{stderr}");
    let bullet = "\n- Module headers are excluded from the count.";
    assert_eq!(stderr.contains(bullet), explains, "{stderr}");
}
