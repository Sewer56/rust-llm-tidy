//! TEXT010 documentation-context detection through the CLI with isolated config.

use super::common::binary;
use super::temp_dir;
use rstest::rstest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Markdown with a blank opening line, so the anchor must be line 2.
const BLANK_OPENER: &str = "\n# Guide\n\nBody text.\n";

/// `--checks-only` reports the reminder without rewriting the file.
#[test]
fn checks_only_should_report_documentation_context_without_writing() {
    let source = "\n# Title\n\n| Name | Value |\n| --- | --- |\n| long | 1 |\n";
    let files = [("README.MD", source)];

    let records = audit(&files, "README.MD", &["--checks-only"]);

    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["line"], 2);
    assert_eq!(records[0]["code"], "TEXT010");
}

/// Detected files report one reminder naming the evidence.
#[rstest]
#[case::readme("README.MD", &[], "README filename")]
#[case::quickstart("QUICKSTART.md", &[], "QUICKSTART filename")]
#[case::getting_started("GETTING_STARTED.mdx", &[], "GETTING_STARTED filename")]
#[case::docs("docs/setup.md", &[], "file is under docs/")]
#[case::docs_case("Docs/Setup.MARKDOWN", &[], "file is under docs/")]
#[case::mkdocs("guide.md", &[("mkdocs.yml", "site_name: docs\n")], "nearby mkdocs.yml")]
#[case::docusaurus(
    "guide.md",
    &[("docusaurus.config.ts", "export default {};\n")],
    "nearby docusaurus.config.ts"
)]
#[case::vitepress(
    "guide.md",
    &[(".vitepress/config.ts", "export default {};\n")],
    "nearby .vitepress"
)]
#[case::mdbook("guide.md", &[("book.toml", "[book]\n")], "nearby book.toml")]
#[case::antora("guide.md", &[("antora.yml", "name: docs\n")], "nearby antora.yml")]
#[case::readthedocs(
    "guide.md",
    &[(".readthedocs.yaml", "version: 2\n")],
    "nearby .readthedocs.yaml"
)]
fn cli_should_report_documentation_context(
    #[case] target: &str,
    #[case] markers: &[(&str, &str)],
    #[case] reason: &str,
) {
    let files = files_for(target, markers);

    let records = audit(&files, target, &["--checks-only"]);

    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["code"], "TEXT010");
    assert_eq!(records[0]["severity"], "ai_reminder");
    assert_eq!(records[0]["title"], "documentation audience review");
    assert_eq!(records[0]["item_kind"], "file");
    assert_eq!(records[0]["item_name"], serde_json::Value::Null);
    assert_eq!(records[0]["line"], 2);
    let message = records[0]["message"].as_str().unwrap();
    assert!(message.contains(reason), "{message}");
    assert!(message.contains("\nWhy: "), "{message}");
    assert!(message.contains("\nSuggestions:\n  - "), "{message}");
}

/// Files without a signal stay silent.
#[rstest]
#[case::agents("docs/AGENTS.md", &[])]
#[case::agent_case("AGENTS.MD", &[])]
#[case::non_prose("docs/example.rs", &[])]
#[case::similar_directory("mydocs/notes.md", &[])]
#[case::similar_file("docs.md", &[])]
fn cli_should_stay_silent_without_a_signal(#[case] target: &str, #[case] markers: &[(&str, &str)]) {
    let files = files_for(target, markers);

    let records = audit(&files, target, &["--checks-only"]);

    assert!(records.is_empty(), "{records:?}");
}

/// Replace each record's displayed path, which legitimately follows the input
/// spelling, so relative and absolute runs compare as records.
fn normalized(records: &[serde_json::Value]) -> Vec<serde_json::Value> {
    records
        .iter()
        .map(|record| {
            let mut record = record.clone();
            record["path"] = serde_json::Value::String("<target>".to_string());
            record
        })
        .collect()
}

/// Relative selections walk the real ancestors, not just the working
/// directory: a marker above cwd and a cwd named `docs` both match their
/// absolute spelling.
#[rstest]
#[case::marker_above_cwd(
    "chapter",
    "guide.md",
    &[("mkdocs.yml", "site_name: docs\n"), ("chapter/guide.md", BLANK_OPENER)],
    "nearby mkdocs.yml"
)]
#[case::cwd_named_docs(
    "docs",
    "setup.md",
    &[("docs/setup.md", BLANK_OPENER)],
    "file is under docs/"
)]
fn relative_selections_should_match_absolute_selections(
    #[case] cwd: &str,
    #[case] target: &str,
    #[case] files: &[(&str, &str)],
    #[case] reason: &str,
) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    for (relative, content) in files {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }
    let config = write_config(&dir);
    let absolute = dir.join(cwd).join(target);

    let relative_run = run_from(&dir, cwd, target, &config);
    let relative_bytes = fs::read(&absolute).unwrap();
    assert_eq!(relative_bytes, BLANK_OPENER.as_bytes());
    let absolute_run = run_from(&dir, cwd, absolute.to_str().unwrap(), &config);
    let absolute_bytes = fs::read(&absolute).unwrap();
    assert_eq!(absolute_bytes, BLANK_OPENER.as_bytes());
    assert_eq!(relative_bytes, absolute_bytes);

    // Both spellings identify the same file and must agree on the finding.
    for (records, exit) in [&relative_run, &absolute_run] {
        assert_eq!(*exit, 0, "{records:?}");
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0]["code"], "TEXT010");
        assert_eq!(records[0]["line"], 2);
        assert!(
            records[0]["message"].as_str().unwrap().contains(reason),
            "{records:?}"
        );
    }
    assert_eq!(
        normalized(&relative_run.0),
        normalized(&absolute_run.0),
        "relative and absolute selections must report identical records"
    );
    let content = fs::read_to_string(&absolute).unwrap();
    assert_eq!(
        content, BLANK_OPENER,
        "checks-only must not rewrite the file"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Audit `target` in a fresh temp tree with empty config and JSON output.
///
/// Uses `--all-lines`, so reminders report without Git eligibility, and
/// `--include TEXT010`, so nothing else can contribute records.
fn audit(files: &[(&str, &str)], target: &str, extra_args: &[&str]) -> Vec<serde_json::Value> {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    for (relative, content) in files {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }
    let config = write_config(&dir);

    let output = Command::new(binary())
        .current_dir(&dir)
        .arg("--config")
        .arg(&config)
        .args(["--include", "TEXT010", "--all-lines", "--json"])
        .args(extra_args)
        .arg(target)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {target}: {e}"));

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    if extra_args.contains(&"--checks-only") {
        let expected = files
            .iter()
            .find(|(relative, _)| *relative == target)
            .map(|(_, content)| *content)
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(target)).unwrap(),
            expected,
            "checks-only must not rewrite {target}"
        );
    }
    let _ = fs::remove_dir_all(&dir);

    records
}

/// `files` with the target appended when marker cases name it only once.
fn files_for<'a>(target: &'a str, markers: &'a [(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut files: Vec<(&str, &str)> = markers.to_vec();
    if !files.iter().any(|(relative, _)| *relative == target) {
        files.push((target, BLANK_OPENER));
    }
    files
}

/// Run `target` from the fixture subdirectory `cwd`, returning parsed records
/// and the exit code.
fn run_from(dir: &Path, cwd: &str, target: &str, config: &Path) -> (Vec<serde_json::Value>, i32) {
    let output = Command::new(binary())
        .current_dir(dir.join(cwd))
        .arg("--config")
        .arg(config)
        .args([
            "--include",
            "TEXT010",
            "--all-lines",
            "--checks-only",
            "--json",
        ])
        .arg(target)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn rust-llm-tidy on {target}: {e}"));
    let records: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    (records, output.status.code().unwrap_or(-1))
}

/// Write the empty config these runs rely on and return its path.
fn write_config(dir: &Path) -> PathBuf {
    let config = dir.join("config.yml");
    fs::write(&config, "{}\n").unwrap();
    config
}
