//! Fixes leave string literals and configuration bytes untouched.
//!
//! Covers raw/verbatim literals across languages, config files, and the
//! borrowed-restore behavior when only one pass changes the file. The shared
//! runner helpers live in `mod.rs`.

use super::common::binary;
use super::{fixture_dir, run_command, temp_file};
use rstest::rstest;
use std::fs;
use std::process::Command;

/// Markdown that would need all three fixes if it were prose, not literal data.
const LITERAL_MARKDOWN: &str = "\
  | a | b |\n  |---|---|\n  | long value | c |\n\n\
  \x20 [A](https://example.invalid)\n\n\
  \x20 ~~~markdown\n  ~~~rust\n  code\n  ~~~\n  ~~~\n";

/// Verified doc comments still hoist links beside an untouched raw literal.
#[rstest]
fn cli_should_fix_doc_comments_when_a_raw_literal_is_adjacent(
    #[values(false, true)] dry_run: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let literal = format!("const PAYLOAD: &str = r#\"\n{LITERAL_MARKDOWN}\"#;\n");
    let source = format!("/// See [A](https://example.invalid).\npub struct A;\n{literal}");
    fs::write(&path, &source).unwrap();
    let args: &[&str] = if dry_run {
        &["--include", "links", "--dry-run", "--json"]
    } else {
        &["--include", "links", "--json"]
    };

    let output = run_command(args, &path);
    let consumed = fs::read(&path).unwrap();

    let expected = if dry_run {
        source
    } else {
        format!("/// See [A].\n///\n/// [A]: https://example.invalid\npub struct A;\n{literal}")
    };
    assert_eq!(consumed, expected.as_bytes());
    let records: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        records
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["code"] == "FIX")
            .count(),
        1,
        "only the doc-comment link should report a fix: {records}"
    );
}

/// Fixes leave configuration and literals byte-exact, including dry runs.
#[rstest]
#[case::toml("payload = \"\"\"\n", "\"\"\"\n", "toml")]
#[case::yaml("payload: |\n", "", "yaml")]
#[case::yml("payload: |\n", "", "yml")]
#[case::rust_raw("const PAYLOAD: &str = r#\"\n", "\"#;\n", "rs")]
#[case::python_multiline("payload = \"\"\"\n", "\"\"\"\n", "py")]
#[case::python_stub_multiline("payload = \"\"\"\n", "\"\"\"\n", "pyi")]
#[case::csharp_verbatim("class C { string payload = @\"\n", "\"; }\n", "cs")]
#[case::csharp_raw("class C { string payload = \"\"\"\n", "\"\"\"; }\n", "cs")]
#[case::shell_heredoc("cat <<'PAYLOAD'\n", "PAYLOAD\n", "sh")]
#[case::javascript_template("const payload = `\n", "`;\n", "js")]
fn cli_should_preserve_configuration_and_literal_values(
    #[case] open: &str,
    #[case] close: &str,
    #[case] extension: &str,
    #[values(false, true)] explicit_fixes: bool,
    #[values(false, true)] dry_run: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("input.{extension}"));
    let config = directory.path().join(".rust-llm-tidy.yml");
    fs::write(&config, "{}\n").unwrap();
    let source = format!("{open}{LITERAL_MARKDOWN}{close}");
    fs::write(&path, &source).unwrap();

    let mut command = Command::new(binary());
    command.arg("--config").arg(config).arg("--json");
    if explicit_fixes {
        command.args([
            "--include",
            "tables",
            "--include",
            "fences",
            "--include",
            "links",
        ]);
    }
    if dry_run {
        command.arg("--dry-run");
    }

    let output = command.arg(&path).output().unwrap();
    let consumed = fs::read(path).unwrap();

    assert_eq!(consumed, source.as_bytes());
    let records: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Default lint diagnostics, including TEXT warnings, do not authorize edits.
    assert!(
        records
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["code"] != "FIX"),
        "literal data must not report fixes: {records}"
    );
}

/// Default all-pass `fix` on a file where only the table changes.
///
/// The later fence/link passes are no-ops and restore `prior`, so the earlier
/// table fix must survive and produce one record plus a byte-identical write.
#[test]
fn fix_default_passes_borrowed_restore_preserves_earlier_change() {
    let before = fixture_dir().join("table_md_before.md");
    let expected = fs::read_to_string(fixture_dir().join("table_md_after.md")).unwrap();
    let tmp = temp_file("md");
    fs::write(&tmp, fs::read_to_string(&before).unwrap()).unwrap();

    let output = run_command(&[], &tmp); // default: tables, fences, links
    assert!(
        output.status.success(),
        "default fix should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&tmp).unwrap();
    let _ = fs::remove_file(&tmp);
    assert_eq!(
        actual, expected,
        "fences/links no-op restore must keep the table fix"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("success[FIX]").count(),
        1,
        "only the realigned table reports a record: {stderr}"
    );
}
