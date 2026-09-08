//! Cross-language MOD001 acceptance through the CLI.

use super::common::binary;
use rstest::rstest;
use std::fs;
use std::process::{Command, Output};

/// Run an isolated file with explicit configuration and automatic fixture cleanup.
pub(super) fn run(source: &str, relative_path: &str, config: &str, args: &[&str]) -> Output {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    let config_path = dir.path().join(".rust-llm-tidy.yml");
    fs::write(&config_path, config).unwrap();

    Command::new(binary())
        .arg("--config")
        .arg(config_path)
        .args(args)
        .arg(path)
        .output()
        .unwrap()
}

// Non-code and test opt-ins.

/// Including test directories preserves the independent inline-test policy.
#[rstest]
#[case::root_tests("tests/source.rs")]
#[case::nested_tests("src/tests/nested/source.rs")]
#[case::singular_directory("test/source.rs")]
#[case::similar_directory("integration_tests/source.rs")]
#[case::file_named_tests("src/tests.rs")]
fn mod001_should_apply_inline_test_policy_when_test_files_are_enabled(
    #[case] path: &str,
    #[values(false, true)] include_in_file_tests: bool,
) {
    let config = format!(
        "module_size:\n  max_lines: 1\n  include_test_files: true\n  include_in_file_tests: {include_in_file_tests}\n"
    );

    let output = run(
        "fn value() {}\n#[cfg(test)]\nmod tests {}\n",
        path,
        &config,
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(
        stderr.matches("warning[MOD001]").count(),
        usize::from(include_in_file_tests),
        "{stderr}"
    );
    if include_in_file_tests {
        assert!(stderr.contains(":2: warning[MOD001]"), "{stderr}");
        assert!(stderr.contains("file has 3 lines,"), "{stderr}");
    }
}

/// Each switch affects only its own category; non-Rust inline tests always count.
#[rstest]
#[case::neither(false, false)]
#[case::non_code_only(true, false)]
#[case::inline_tests_only(false, true)]
#[case::both(true, true)]
fn mod001_should_apply_size_options_independently(
    #[values(
        ("source.rs", "fn value() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn example() {}\n}\n"),
        ("source.yaml", "value: 1\n\n"),
        ("source.py", "def test_value():\n    assert True\n"),
        ("tests/source.rs", "fn value() {}\nfn other() {}\n#[cfg(test)]\nmod tests {}\n")
    )]
    fixture: (&str, &str),
    #[case] include_non_code: bool,
    #[case] include_in_file_tests: bool,
    #[values(false, true)] include_test_files: bool,
) {
    let config = format!(
        "module_size:\n  max_lines: 1\n  include_non_code: {include_non_code}\n  include_in_file_tests: {include_in_file_tests}\n  include_test_files: {include_test_files}\n"
    );
    let (path, source) = fixture;
    let warns = match path {
        "source.rs" => include_in_file_tests,
        "source.yaml" => include_non_code,
        "source.py" => true,
        "tests/source.rs" => include_test_files,
        _ => unreachable!(),
    };

    let output = run(source, path, &config, &["--include", "MOD001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("warning[MOD001]"), warns, "{stderr}");
    if path == "source.rs" && include_in_file_tests {
        assert!(
            stderr.contains(&format!("file has {} lines,", source.lines().count())),
            "{stderr}"
        );
        assert!(!stderr.contains("outside `#[cfg(test)]`"), "{stderr}");
    }
}

/// The non-code opt-in covers supported formats, never unknown extensions.
#[rstest]
#[case::yaml("yaml", true)]
#[case::yml("yml", true)]
#[case::toml("toml", true)]
#[case::conf("conf", true)]
#[case::jsonc("jsonc", true)]
#[case::json5("json5", true)]
#[case::powershell_data("psd1", true)]
#[case::latex("tex", true)]
#[case::latex_short("ltx", true)]
#[case::latex_style("sty", true)]
#[case::latex_class("cls", true)]
#[case::bibtex_style("bst", true)]
#[case::markdown("md", true)]
#[case::markdown_long("markdown", true)]
#[case::mdx("mdx", true)]
#[case::plaintext("txt", true)]
#[case::text("text", true)]
#[case::ini("ini", true)]
#[case::json("json", true)]
#[case::unknown("unknown", false)]
fn mod001_should_check_supported_non_code_when_enabled(
    #[case] extension: &str,
    #[case] warns: bool,
) {
    let path = format!("source.{extension}");

    let output = run(
        "\n\n",
        &path,
        "module_size:\n  max_lines: 1\n  include_non_code: true\n",
        &["--include", "MOD001", "--extension", extension],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains(":2: warning[MOD001]"), warns, "{stderr}");
}

/// Item documentation and comments after code are not headers.
#[rstest]
#[case::rust_item_docs("source.rs", "/// Item doc.\nfn value() {}\nfn other() {}\n", 2, 3)]
#[case::rust_body_comment("source.rs", "fn value() {}\n// Body.\nfn other() {}\n", 2, 3)]
#[case::python_nested_docstring("source.py", "def a():\n    \"\"\"Nested.\"\"\"\n", 2, 2)]
fn mod001_should_count_item_docs_and_body_comments(
    #[case] path: &str,
    #[case] source: &str,
    #[case] crossing: usize,
    #[case] counted: usize,
) {
    let output = run(
        source,
        path,
        "module_size:\n  max_lines: 1\n",
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains(&format!(":{crossing}: warning[MOD001]")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("file has {counted} lines")),
        "{stderr}"
    );
}

/// Disabling the exclusion counts header lines again with the plain message.
#[rstest]
#[case::rust("source.rs", "//! Docs.\n")]
#[case::python("source.py", "\"\"\"Doc.\"\"\"\n")]
fn mod001_should_count_module_headers_when_exclusion_is_disabled(
    #[case] path: &str,
    #[case] header: &str,
) {
    let source = format!("{header}value = 1\n");

    let output = run(
        &source,
        path,
        "module_size:\n  max_lines: 1\n  exclude_module_headers: false\n",
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(":2: warning[MOD001]"), "{stderr}");
    assert!(stderr.contains("file has 2 lines"), "{stderr}");
}

// Rendered guidance and rule selection.

/// The rendered warning guides modular design without encouraging arbitrary splits.
#[rstest]
#[case::rust_without_tests("source.rs", false, " outside `#[cfg(test)]` mod regions")]
#[case::rust_with_tests("source.rs", true, "")]
#[case::python("source.py", false, "")]
#[case::csharp("source.cs", false, "")]
fn mod001_should_explain_focused_module_boundaries(
    #[case] path: &str,
    #[case] include_in_file_tests: bool,
    #[case] exclusions: &str,
) {
    let config =
        format!("module_size:\n  max_lines: 1\n  include_in_file_tests: {include_in_file_tests}\n");
    let mut expected = indoc::formatdoc! {"
        warning[MOD001]: file has 2 lines{exclusions},
        over the 1-line budget (module_size.max_lines).
        Why:
        - Large files make readers search farther and keep more context in mind.
        - A split should make responsibilities easier to find, not just shorten files.
        Suggestions:
        - Consider keeping entry points and orchestration near the top level, with
          implementation details in focused child modules.
        - Group code by responsibility, such as parsing or validation. Domain names
          usually explain more than catch-all names like `utils`.
        - Let orchestration read as calls to clear operations, such as `parse_imports`
          or `validate_config`.
        - Free functions suit stateless work. Methods suit behavior that manages a
          type's state.
        - Keep closely related code together. A split need not add new types,
          forwarding wrappers, or a wider public API.
        - Update overview docs to explain responsibilities and point readers to the
          entry points. Keep useful documentation; a split should not remove it."};
    if path.ends_with(".rs") {
        let regions = if include_in_file_tests {
            "counted"
        } else {
            "excluded"
        };

        expected.push_str(&indoc::formatdoc! {"

            - In Rust, the module root (`mod.rs` or `foo.rs`) is the usual home for
              those entry points.
            - Selective re-exports can expose child-module entry points without a
              forwarding wrapper.
            Counting:
            - Top-level `#[cfg(test)]` test modules are {regions}.
              Other lines count, including comments and blank lines.
            - Rust files in `tests/` directories are skipped."});
    }
    expected.push_str(" (file)");

    let output = run("\n\n", path, &config, &["--include", "MOD001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(&expected), "{stderr}");
}

/// Rust warnings explain test counting without exposing configuration keys.
#[rstest]
#[case::defaults(false, false, "excluded", "skipped")]
#[case::inline_tests(true, false, "counted", "skipped")]
#[case::test_files(false, true, "excluded", "checked")]
#[case::all_tests(true, true, "counted", "checked")]
fn mod001_should_explain_test_counting_in_plain_language(
    #[case] include_in_file_tests: bool,
    #[case] include_test_files: bool,
    #[case] regions: &str,
    #[case] files: &str,
) {
    let config = format!(
        "module_size:\n  max_lines: 1\n  include_in_file_tests: {include_in_file_tests}\n  include_test_files: {include_test_files}\n"
    );

    let output = run("\n\n", "source.rs", &config, &["--include", "MOD001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "Top-level `#[cfg(test)]` test modules are {regions}.\n  \
             Other lines count, including comments and blank lines."
        )),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("Rust files in `tests/` directories are {files}.")),
        "{stderr}"
    );
    assert!(!stderr.contains("module_size.include_"), "{stderr}");
}

/// The file-level rule follows the same CLI selection as other lint codes.
#[rstest]
#[case::default_run(&["--dry-run"], true)]
#[case::lints_group(&["--include", "lints"], true)]
#[case::code_only(&["--include", "MOD001"], true)]
#[case::excluded_code(&["--exclude", "MOD001"], false)]
#[case::excluded_group(&["--exclude", "lints"], false)]
#[case::other_code_only(&["--include", "TEXT001"], false)]
fn mod001_should_follow_cli_rule_selection(#[case] args: &[&str], #[case] warns: bool) {
    let output = run("\n\n", "source.js", "module_size:\n  max_lines: 1\n", args);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("warning[MOD001]"), warns, "{stderr}");
}

/// Config suppression disables only the selected size rule.
#[rstest]
#[case::included("include:\n  - rules: [MOD001]\n", true)]
#[case::excluded("exclude:\n  - rules: [MOD001]\n", false)]
#[case::excluded_group("exclude:\n  - rules: [lints]\n", false)]
fn mod001_should_follow_config_rule_selection(#[case] selection: &str, #[case] warns: bool) {
    let config = format!("module_size:\n  max_lines: 1\n{selection}");

    let output = run("\n\n", "source.js", &config, &["--dry-run"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("warning[MOD001]"), warns, "{stderr}");
}

// Thresholds, defaults, and language eligibility.

/// A configured budget counts code, comments, blanks, and inline test code alike.
#[rstest]
#[case::empty("", false)]
#[case::below_budget("value = 1\n", false)]
#[case::at_budget("value = 1\n# comment\n", false)]
#[case::over_budget("value = 1\n# comment\n\n", true)]
#[case::unterminated_line("value = 1\n# comment\nvalue = 2", true)]
#[case::crlf("value = 1\r\n# comment\r\n\r\n", true)]
#[case::inline_tests("value = 1\ndef test_value():\n    assert value == 1\n", true)]
fn mod001_should_follow_the_configured_physical_line_budget(
    #[case] source: &str,
    #[case] warns: bool,
) {
    let output = run(
        source,
        "source.py",
        "module_size:\n  max_lines: 2\n",
        &["--include", "MOD001"],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains(":3: warning[MOD001]"), warns, "{stderr}");
    assert_eq!(
        stderr.matches("MOD001").count(),
        usize::from(warns),
        "{stderr}"
    );
}

/// Opting in does not bypass extension selection or rule and path exclusions.
#[rstest]
#[case::selected("extensions: [json]\n", &["--include", "MOD001"], true)]
#[case::lints_group("extensions: [json]\n", &["--include", "lints"], true)]
#[case::extra_extension("extra_extensions: [json]\n", &["--include", "MOD001"], true)]
#[case::unselected("", &["--include", "MOD001"], false)]
#[case::replaced_extensions("extensions: [rs]\n", &["--include", "MOD001"], false)]
#[case::cli_extension("", &["--include", "MOD001", "--extension", "json"], true)]
#[case::excluded_file("extensions: [json]\nexclude_files: [source.json]\n", &[], false)]
#[case::excluded_rule("extensions: [json]\nexclude:\n  - rules: [MOD001]\n", &[], false)]
#[case::excluded_group("extensions: [json]\n", &["--exclude", "lints"], false)]
#[case::included_rule("extensions: [json]\ninclude:\n  - rules: [MOD001]\n", &[], true)]
#[case::other_rule("extensions: [json]\n", &["--include", "TEXT001"], false)]
fn mod001_should_respect_selection_when_non_code_is_enabled(
    #[case] selection: &str,
    #[case] args: &[&str],
    #[case] warns: bool,
) {
    let config = format!("module_size:\n  max_lines: 1\n  include_non_code: true\n{selection}");

    let output = run("{}\n\n", "source.json", &config, args);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.contains("warning[MOD001]"), warns, "{stderr}");
}

/// Non-code formats remain exempt by default even when explicitly selected.
#[rstest]
#[case::markdown("source.md")]
#[case::markdown_long_extension("source.markdown")]
#[case::mdx("source.mdx")]
#[case::plaintext("source.txt")]
#[case::text("source.text")]
#[case::json("source.json")]
#[case::ini("source.ini")]
#[case::yaml("source.yaml")]
#[case::yml("source.yml")]
#[case::toml("source.toml")]
#[case::conf("source.conf")]
#[case::jsonc("source.jsonc")]
#[case::json5("source.json5")]
#[case::powershell_data("source.psd1")]
#[case::latex("source.tex")]
#[case::latex_short("source.ltx")]
#[case::latex_style("source.sty")]
#[case::latex_class("source.cls")]
#[case::bibtex_style("source.bst")]
#[case::unmapped("source.unknown")]
fn mod001_should_skip_non_code_by_default(#[case] path: &str) {
    let ext = path.rsplit('.').next().unwrap();

    let output = run(
        "\n\n",
        path,
        "module_size:\n  max_lines: 1\n",
        &["--include", "MOD001", "--extension", ext],
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("MOD001"), "{stderr}");
}

/// An absent section keeps non-code files outside the default size budget.
#[test]
fn mod001_should_skip_non_code_when_configuration_section_is_absent() {
    let defaults: rust_llm_tidy::config::ModuleSizeConfig = serde_json::from_str("{}").unwrap();
    let source = "\n".repeat(defaults.max_lines + 1);

    let output = run(&source, "source.yaml", "{}", &["--include", "MOD001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("MOD001"), "{stderr}");
}

/// Backend and backendless languages count all lines, including files under tests/.
#[rstest]
#[case::csharp("source.cs")]
#[case::python("source.py")]
#[case::python_stub("source.pyi")]
#[case::javascript("source.js")]
#[case::typescript("source.ts")]
#[case::tsx("source.tsx")]
#[case::go("source.go")]
#[case::java("source.java")]
#[case::zig("source.zig")]
#[case::shell("source.sh")]
#[case::uppercase_extension("source.PY")]
#[case::python_tests("tests/test_source.py")]
#[case::javascript_tests("tests/source.test.js")]
fn mod001_should_warn_once_for_oversized_code(#[case] path: &str) {
    let defaults: rust_llm_tidy::config::ModuleSizeConfig = serde_json::from_str("{}").unwrap();
    let over_budget = defaults.max_lines + 1;
    let source = "\n".repeat(over_budget);

    let output = run(&source, path, "{}", &["--include", "MOD001"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("warning[MOD001]").count(), 1, "{stderr}");
    assert!(
        stderr.contains(&format!(":{over_budget}: warning[MOD001]")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("file has {over_budget} lines,\n")),
        "{stderr}"
    );
    assert!(!stderr.contains("#[cfg(test)]"), "{stderr}");
}

/// Data-file opt-in does not enable transformations or text diagnostics.
#[rstest]
#[case::ini("ini")]
#[case::json("json")]
fn pipeline_should_preserve_data_operations_when_non_code_is_enabled(#[case] extension: &str) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(format!("source.{extension}"));
    let source = "| a | b |\n| --- | --- |\n| 1 | 22 |\nSee [guide](https://example.com).\nTODO: fix this.\n";
    fs::write(&path, source).unwrap();
    let config = dir.path().join(".rust-llm-tidy.yml");
    fs::write(
        &config,
        format!(
            "extensions: [{extension}]\nmodule_size:\n  max_lines: 1\n  include_non_code: true\n"
        ),
    )
    .unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(config)
        .arg(&path)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("warning[MOD001]"), "{stderr}");
    assert!(!stderr.contains("TEXT"), "{stderr}");
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}
