//! TEXT010 documentation-context reminders through the no-args Git path.

use super::{cleanup, git, init_repo, run};
use rstest::rstest;
use std::fs;

/// Baseline and edited bodies share line 1, so only the intended lines differ.
const BASELINE: &str = "# Title\n\nBaseline.\n";
const CURRENT: &str = "# Title\n\nIntro.\n\nMore.\n";

/// The first changed line carries the single reminder, whatever the signal.
#[rstest]
#[case::readme("README.MD", false, "README filename", 3)]
#[case::docs("docs/setup.md", false, "file is under docs/", 3)]
#[case::marker("guide.md", true, "nearby mkdocs.yml", 3)]
fn reminder_should_anchor_at_the_first_changed_line(
    #[case] target: &str,
    #[case] marker: bool,
    #[case] reason: &str,
    #[case] line: u64,
) {
    let repo = init_repo().expect("Git is required for TEXT010 acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}\n").unwrap();
    if marker {
        fs::write(repo.join("mkdocs.yml"), "site_name: docs\n").unwrap();
    }
    let path = repo.join(target);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, BASELINE).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(&path, CURRENT).unwrap();

    let output = run(&repo, &["--include", "TEXT010", "--json"]);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["code"], "TEXT010");
    assert_eq!(findings[0]["severity"], "reminder");
    assert_eq!(findings[0]["line"], line);
    assert_eq!(findings[0]["item_kind"], "file");
    assert!(
        findings[0]["message"].as_str().unwrap().contains(reason),
        "{findings:?}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), CURRENT);
    cleanup(&repo);
}

/// The run discovers a new documentation file, staged or untracked, with all
/// lines eligible.
#[rstest]
#[case::untracked(false)]
#[case::staged(true)]
fn reminder_should_cover_new_documentation_files(#[case] staged: bool) {
    let repo = init_repo().expect("Git is required for TEXT010 acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    let path = repo.join("docs/new-guide.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "# New\n\nBody text.\n").unwrap();
    if staged {
        git(&repo, &["add", "docs/new-guide.md"]);
    }

    let output = run(&repo, &["--include", "TEXT010", "--json"]);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["line"], 1);
    assert!(
        findings[0]["message"]
            .as_str()
            .unwrap()
            .contains("file is under docs/"),
        "{findings:?}"
    );
    cleanup(&repo);
}

/// Whole-file audits and rule selection follow the existing conventions.
///
/// The all-lines row asserts anchor line 1. Default changed-line scope would
/// anchor at line 3, so both scopes report one finding and the anchor alone
/// proves the flag took effect.
#[rstest]
#[case::all_lines(&["--include", "TEXT010", "--all-lines", "--json"], 1, Some(1))]
#[case::excluded(&["--include", "TEXT010", "--exclude", "TEXT010", "--json"], 0, None)]
#[case::unrelated_rule(&["--include", "TEXT002", "--json"], 0, None)]
fn reminder_should_follow_all_lines_and_rule_selection(
    #[case] args: &[&str],
    #[case] count: usize,
    #[case] expected_line: Option<u64>,
) {
    let repo = init_repo().expect("Git is required for TEXT010 acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}\n").unwrap();
    fs::write(repo.join("README.MD"), BASELINE).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(repo.join("README.MD"), CURRENT).unwrap();

    let output = run(&repo, args);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(findings.len(), count, "{findings:?}");
    if let Some(line) = expected_line {
        assert_eq!(findings[0]["line"], line);
    }
    cleanup(&repo);
}
